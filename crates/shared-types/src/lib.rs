//! Types shared between the Tauri command layer, core-manager, net, and the
//! privileged helpers. This is the single source of truth for the wire
//! format between the Rust backend and the React frontend (mirrored on the
//! TS side under `ui/src/ipc/types.ts` — keep the two in sync until we wire
//! up `specta` codegen).
//!
//! Scope is intentionally MVP-sized: enough fields to start/stop a proxy,
//! list/edit servers, and drive the three privileged helpers. Full protocol
//! coverage (the ~15 protocols/transports the Electron version supports) is
//! deferred to a follow-up pass — see README "MVP scope". Vless/Trojan/
//! Shadowsocks/Vmess/Wireguard are in scope; Wireguard is manual-entry-only
//! (no subscription-link format for it, see `crates/subscription`) and has
//! no TLS layer of its own (see `ServerConfig::wireguard_*` fields below).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Vless,
    Trojan,
    Shadowsocks,
    Vmess,
    Wireguard,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TlsConfig {
    pub enabled: bool,
    pub server_name: Option<String>,
    pub insecure: bool,
    pub reality_public_key: Option<String>,
    pub reality_short_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub id: String,
    pub name: String,
    pub protocol: Protocol,
    /// For `Protocol::Wireguard`, this is the peer's endpoint host -- same
    /// field, no dedicated `wireguard_*` address field needed.
    pub address: String,
    /// For `Protocol::Wireguard`, this is the peer's endpoint port.
    pub port: u16,

    pub uuid: Option<String>,
    pub password: Option<String>,
    pub encryption: Option<String>,
    pub flow: Option<String>,

    /// TLS config. Not applicable to `Protocol::Wireguard` -- WireGuard has
    /// its own crypto handshake (see `wireguard_*` fields below) and no TLS
    /// wrapping at all in sing-box's WireGuard support.
    pub tls: Option<TlsConfig>,

    /// Base64-encoded 32-byte Curve25519 private key for this client, used
    /// only by `Protocol::Wireguard`.
    pub wireguard_private_key: Option<String>,
    /// Base64-encoded 32-byte Curve25519 public key of the remote peer,
    /// used only by `Protocol::Wireguard`.
    pub wireguard_peer_public_key: Option<String>,
    /// Optional base64-encoded 32-byte pre-shared key, used only by
    /// `Protocol::Wireguard`. `None` when the peer doesn't use one.
    pub wireguard_pre_shared_key: Option<String>,
    /// This client's local tunnel address in CIDR form (e.g. `10.0.0.2/32`),
    /// used only by `Protocol::Wireguard`. sing-box's own WireGuard `address`
    /// field is an array (to support e.g. one IPv4 + one IPv6 address at
    /// once); this app's MVP scope is a single address, wrapped into a
    /// one-element array when building the config (see
    /// `core-manager::config::build_outbound`'s `Wireguard` arm).
    pub wireguard_local_address: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProxyMode {
    Global,
    Smart,
    Direct,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProxyModeType {
    SystemProxy,
    Tun,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProxyErrorCode {
    HelperUnavailable,
    CoreStartFailed,
    PortInUse,
    ConfigInvalid,
    Unknown,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStatus {
    pub running: bool,
    pub pid: Option<u32>,
    /// Unix millis, matches `Date.now()` on the TS side.
    pub start_time: Option<i64>,
    pub uptime_secs: Option<u64>,
    pub error: Option<String>,
    pub error_code: Option<ProxyErrorCode>,
    pub current_server_id: Option<String>,
    /// The local `mixed` (HTTP+SOCKS) inbound's port, when this run has one
    /// (`SystemProxy`/`Manual` modes). `None` for `Tun` mode, which has no
    /// local inbound to report, and whenever nothing is running. Surfaced
    /// mainly for `Manual` mode, where the user needs this to point their
    /// own apps at the proxy by hand.
    pub local_port: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleMatchType {
    Domain,
    DomainSuffix,
    DomainKeyword,
    IpCidr,
    ProcessName,
    /// References one or more already-downloaded GeoIP/GeoSite `.srs`
    /// rule-set resources by id (see [`RuleResourceInfo::id`]) instead of a
    /// literal domain/IP/process value -- see [`RoutingRule::values`]'s doc
    /// comment for the dual meaning this gives that field, and
    /// `core_manager::config::build_route_rules` for how this maps to
    /// sing-box's `route.rule_set` + `{"rule_set": [...], "outbound": ...}`
    /// shape.
    RuleSet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleOutbound {
    Proxy,
    Direct,
    Block,
}

/// `rename_all = "camelCase"` here is load-bearing, not cosmetic: unlike
/// this struct's `values`/`outbound` fields, `match_type` is a *required*
/// (non-`Option`) field with no default, so without this attribute the
/// literal wire key would be `match_type` -- and the frontend always sends
/// `matchType` (see `src/types.ts`'s `RoutingRule`) when constructing a
/// fresh rule (`RuleForm`), which isn't a partial-object spread of a
/// previous `rules_add`/`rules_update` response the way `UserConfig`
/// settings toggles are. That mismatch doesn't fail silently the way a
/// missing `Option` field does (serde treats an absent key for `Option<T>`
/// as `None` automatically, with or without `#[serde(default)]`) -- it's a
/// hard `serde` "missing field `match_type`" deserialization error, which
/// surfaces as `rules_add`/`rules_update` unconditionally rejecting every
/// call from the UI. Confirmed via real end-to-end testing while building
/// the rule-resources feature (adding a `RuleSet`-type rule through the
/// actual running app failed with exactly that error before this attribute
/// was added).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub match_type: RuleMatchType,
    /// One or more raw match values -- meaning depends on `match_type`:
    /// - `Domain`/`DomainSuffix`/`DomainKeyword`/`IpCidr`/`ProcessName`:
    ///   literal domains/suffixes/keywords/CIDRs/process names, typed in by
    ///   hand (unchanged from before rule-sets existed).
    /// - `RuleSet`: one or more [`RuleResourceInfo::id`]s of already-
    ///   downloaded GeoIP/GeoSite `.srs` resources (see the `rule-resources`
    ///   crate + `UserConfig::rule_resources`) rather than literal values --
    ///   a rule can only reference resources that have actually been
    ///   downloaded, which is the UI's job to enforce (see `RuleForm`).
    ///
    /// No cross-field validation here either way -- that's the UI's job to
    /// keep the input reasonable.
    pub values: Vec<String>,
    pub outbound: RuleOutbound,
}

/// Which upstream repo/file-prefix a downloaded rule-set resource came from
/// -- mirrors `rule_resources::ResourceCategory` (a distinct type in that
/// crate, since `rule-resources` itself has no need for `shared-types`'
/// wire-format conventions) but is the shape actually persisted in
/// `UserConfig`/sent over IPC. See `RuleResourceInfo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleResourceCategory {
    Geosite,
    GeoIp,
}

/// One downloaded GeoIP/GeoSite `.srs` rule-set resource, tracked in
/// `UserConfig::rule_resources` so a `RoutingRule` with
/// `match_type: RuleMatchType::RuleSet` can reference it by `id` (see that
/// variant's doc comment). The actual `.srs` file lives on disk at a path
/// this struct doesn't carry -- see `state::rule_resources_dir` /
/// `<category>-<name>.srs` naming convention on the `src-tauri` side; this
/// struct is metadata only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleResourceInfo {
    /// Stable id a `RoutingRule.values` entry references -- `"<category
    /// file-prefix>-<name>"` (e.g. `"geosite-netflix"`, `"geoip-cn"`), see
    /// `commands::rule_resources::resource_id` on the `src-tauri` side).
    /// **Not** bare `name` -- the builtin catalog deliberately has entries
    /// that share a `name` across the two `category` values (e.g. `"cn"` is
    /// both a GeoSite and a GeoIP entry), so an id derived from `name` alone
    /// would collide between them: downloading both would upsert the same
    /// `UserConfig.rule_resources` slot twice, silently leaving only one of
    /// the two categories actually tracked/referenceable (confirmed against
    /// real catalog data -- `builtin_catalog()`'s `"cn"`/`GeoIp` and
    /// `"cn"`/`Geosite` entries -- while building the region-preset feature,
    /// which needs both active at once for e.g. "China direct").
    pub id: String,
    /// Bare category name, e.g. `"netflix"` (no `geosite-`/`geoip-` prefix
    /// or `.srs` suffix) -- see `rule_resources::CatalogEntry::name`.
    pub name: String,
    pub category: RuleResourceCategory,
    /// Whether this came from `rule_resources::builtin_catalog()` (true) or
    /// a user-supplied custom URL via `rule_resources_download_custom`
    /// (false).
    pub is_builtin: bool,
    /// The exact URL last used to fetch this resource, after any
    /// GitHub-acceleration-prefix substitution (`UserConfig.github_accel_prefix`)
    /// -- kept for display/debugging, not re-derived from `name`/`category`
    /// at display time.
    pub source_url: String,
    pub size_bytes: u64,
    pub sha256: String,
    /// RFC3339 timestamp string of the last successful download/update.
    pub downloaded_at: String,
}

fn default_rule_resource_auto_update_interval_hours() -> u32 {
    24
}

/// `rename_all = "camelCase"` here is load-bearing, not cosmetic, for the
/// same reason documented on `RoutingRule` above: `config_save`'s `config`
/// parameter deserializes a `UserConfig` straight from the frontend, and
/// several fields here (`proxy_mode`, `proxy_mode_type`, `auto_start`,
/// `silent_start`, `auto_connect`, `minimize_to_tray`) are required
/// (non-`Option`, no `#[serde(default)]`) -- without this attribute every
/// `config_save` call would have hard-failed with a "missing field" error
/// (the frontend always sends `proxyMode`/`autoStart`/etc., never the
/// literal snake_case key), and `config_get`'s *response* would have come
/// back with snake_case keys the frontend's camelCase-typed field accesses
/// (`config.autoStart`) silently read as `undefined` -- worse, since that
/// direction fails silently rather than throwing, `selected_server_id`
/// specifically would round-trip a STALE value on every save (the
/// leftover snake_case key from the previous `config_get` surviving a
/// spread alongside an ignored, differently-cased new key) while the UI
/// reported success. This was never caught earlier because every save
/// path was verified either through a bare browser tab (no real backend,
/// `invoke` always rejects before reaching serde at all) or by checking
/// for uncaught JS exceptions -- a graceful, caught promise rejection (or
/// a silent no-op) doesn't throw one.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserConfig {
    pub servers: Vec<ServerConfig>,
    pub rules: Vec<RoutingRule>,
    pub selected_server_id: Option<String>,
    pub proxy_mode: ProxyMode,
    pub proxy_mode_type: ProxyModeType,
    pub auto_start: bool,
    pub silent_start: bool,
    pub auto_connect: bool,
    pub minimize_to_tray: bool,
    pub language: Option<String>,
    /// `"system"` (default, `None`/absent), `"light"`, or `"dark"` --
    /// mirrors `language`'s loose `Option<String>` typing rather than an
    /// enum, same reasoning: the frontend owns the fixed set of valid
    /// values (src/i18n's `Language` union has its own direct analogue in
    /// a `Theme` union), and a plain string round-trips forward-compatibly
    /// if that set ever grows without needing a backend change. `None`
    /// means "follow the OS", not "light" -- see src/index.css's
    /// `data-theme` handling.
    #[serde(default)]
    pub theme: Option<String>,
    /// Opt-in, off by default: whether `core_manager::history::HistoryRecorder`
    /// should be spawned alongside the next `CoreManager::start()` call to log
    /// finished connections to a local file (see `docs/ipc-contract.md`'s
    /// "Connection history" section). Mirrors the sibling Electron app's own
    /// `ConnectionHistoryService`, which is likewise opt-in with no source
    /// IP/request content logged even when enabled -- a proxy client that
    /// silently kept a log of every destination a user's traffic visited
    /// would be a real privacy regression, so this defaults to `false` and
    /// only takes effect for runs started *after* it's turned on (flipping it
    /// while the proxy is already running does not retroactively start
    /// logging that run -- see `CoreManager::start`).
    pub connection_history_enabled: bool,

    /// Downloaded GeoIP/GeoSite `.srs` rule-set resources -- see
    /// `RuleResourceInfo` and the `rule-resources` crate. `#[serde(default)]`
    /// so a `config.json` persisted before this feature existed still loads
    /// cleanly instead of failing to parse and silently resetting every
    /// other setting back to defaults (see `state::load_persisted_config`).
    /// (No `rename` needed on this or the three fields below -- the
    /// struct-level `rename_all = "camelCase"` above already produces the
    /// same wire name a per-field `rename` would.)
    #[serde(default)]
    pub rule_resources: Vec<RuleResourceInfo>,
    /// Optional GitHub-acceleration mirror prefix (e.g.
    /// `"https://ghproxy.com/"`), prepended verbatim in front of the real
    /// `raw.githubusercontent.com` URL by `rule_resources::resource_url` --
    /// see that function's doc comment. `None`/absent means fetch directly.
    #[serde(default)]
    pub github_accel_prefix: Option<String>,
    /// Opt-in, off by default (mirrors `connection_history_enabled`'s
    /// convention): whether a background task should periodically
    /// re-download every tracked `rule_resources` entry.
    #[serde(default)]
    pub rule_resource_auto_update: bool,
    /// How often the auto-update task wakes, when `rule_resource_auto_update`
    /// is `true`. Defaults to once a day.
    #[serde(default = "default_rule_resource_auto_update_interval_hours")]
    pub rule_resource_auto_update_interval_hours: u32,

    /// Fallback outbound for traffic that matches no enabled `rules` entry
    /// -- sing-box's `route.final`. Before this field existed,
    /// `core_manager::config::build_config_with_inbound` hardcoded
    /// `route.final` to the proxy outbound; that's still this field's
    /// default (`RuleOutbound::Proxy`), so existing behavior is unchanged
    /// for every config that predates this field. Driven by the
    /// region-preset feature (see `RegionPreset` on the frontend side),
    /// which needs a way to express "proxy only these rule-sets, everything
    /// else direct" -- not expressible with `route.rules` alone, since
    /// sing-box has no literal "match everything" rule condition.
    /// `#[serde(default = ...)]` so a `config.json` persisted before this
    /// field existed still loads cleanly with the same implicit behavior it
    /// always had, rather than failing to parse.
    #[serde(default = "default_outbound_proxy")]
    pub default_outbound: RuleOutbound,
}

fn default_outbound_proxy() -> RuleOutbound {
    RuleOutbound::Proxy
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            servers: Vec::new(),
            rules: Vec::new(),
            selected_server_id: None,
            proxy_mode: ProxyMode::Smart,
            proxy_mode_type: ProxyModeType::SystemProxy,
            auto_start: false,
            silent_start: false,
            auto_connect: false,
            minimize_to_tray: true,
            language: None,
            theme: None,
            connection_history_enabled: false,
            rule_resources: Vec::new(),
            github_accel_prefix: None,
            rule_resource_auto_update: false,
            rule_resource_auto_update_interval_hours: default_rule_resource_auto_update_interval_hours(),
            default_outbound: default_outbound_proxy(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HelperPlatform {
    Windows,
    Macos,
    Linux,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperStatus {
    pub platform: HelperPlatform,
    pub installed: bool,
    pub ready: bool,
    pub version: Option<String>,
    pub needs_repair: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemProxyStatus {
    pub enabled: bool,
    pub http_proxy: Option<String>,
    pub https_proxy: Option<String>,
    pub socks_proxy: Option<String>,
    pub bypass_list: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub platform: HelperPlatform,
    pub arch: String,
    pub os_version: String,
    pub is_admin: bool,
}

/// Mirrors the `metadata` object inside one entry of sing-box's Clash API
/// `GET /connections` response. Only the fields this app actually displays
/// are extracted -- sing-box's real metadata object carries several more
/// (`type`, `sourceIP`, `sourcePort`, `dnsMode`, `processPath`, ...) that
/// aren't surfaced here.
///
/// `destination_ip`/`destination_port` map to sing-box's `destinationIP`/
/// `destinationPort` wire fields. `rename_all = "camelCase"` alone would
/// produce `destinationIp` (lowercase `p`) for `destination_ip`, which does
/// NOT match sing-box's actual `destinationIP` key -- that mismatch would
/// silently deserialize to an empty string rather than fail to compile, so
/// it needs an explicit `rename` override. `destination_port` already
/// round-trips correctly under plain `camelCase` (`destinationPort`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionMetadata {
    pub network: String,
    /// May be empty when sing-box couldn't determine a hostname (e.g. a raw
    /// IP connection with no SNI/Host header) -- callers should fall back to
    /// `destination_ip`/`destination_port` for display in that case.
    pub host: String,
    #[serde(rename = "destinationIP")]
    pub destination_ip: String,
    pub destination_port: String,
}

/// One entry of sing-box's Clash API `GET /connections` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub id: String,
    pub metadata: ConnectionMetadata,
    pub upload: u64,
    pub download: u64,
    /// RFC3339 timestamp string (e.g. `"2024-01-15T10:30:00.123456Z"`), kept
    /// as-is rather than parsed into a numeric timestamp -- this is the only
    /// consumer of this value, and parsing it would mean pulling in a
    /// chrono/time dependency for one field.
    pub start: String,
    /// Outbound tag chain the connection is routed through, outermost first.
    pub chains: Vec<String>,
    /// Name of the `route.rules` entry that matched, or empty when it fell
    /// through to `route.final`.
    pub rule: String,
}

/// Full response shape of sing-box's Clash API `GET /connections`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsSnapshot {
    /// Cumulative bytes downloaded since sing-box started -- sing-box's own
    /// semantics, not something this app computes or resets independently.
    pub download_total: u64,
    pub upload_total: u64,
    pub connections: Vec<ConnectionInfo>,
}

/// One persisted, already-finished connection -- same shape as
/// [`ConnectionInfo`] plus an `end` timestamp. Unlike `start` (sing-box's own
/// timestamp, taken verbatim from the Clash API), `end` is generated by this
/// app (`core_manager::history::now_rfc3339`) at the moment the connection is
/// first noticed as gone from the live snapshot -- sing-box itself has no
/// "connection closed" timestamp in its Clash API. See
/// `core_manager::history::HistoryRecorder` for how these get produced, and
/// `docs/ipc-contract.md`'s "Connection history" section for the on-disk
/// format (one JSON line per entry, capped at the most recent 1000) and the
/// opt-in privacy framing (`UserConfig::connection_history_enabled`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub metadata: ConnectionMetadata,
    pub upload: u64,
    pub download: u64,
    pub start: String,
    pub end: String,
    pub chains: Vec<String>,
    pub rule: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// One captured log line, from either this app's own `tracing` events
/// (`source: "app"`, fed by `src-tauri`'s custom `tracing_subscriber::Layer`)
/// or sing-box's child-process stdout/stderr (`source: "core"`, fed by
/// `core_manager::CoreManager::start`'s spawned line-reader tasks) -- both
/// feed into the single in-memory ring buffer `core_manager::logs::LogBuffer`
/// owns. Exposed via the `logs_get`/`logs_clear` Tauri commands; see
/// `docs/ipc-contract.md`'s "Logs" section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    /// RFC3339 timestamp string (`core_manager::history::now_rfc3339`), the
    /// moment this entry was captured -- not necessarily the moment the
    /// underlying event happened (sing-box's own stdout/stderr lines carry
    /// no timestamp this app parses out).
    pub timestamp: String,
    pub level: LogLevel,
    /// `"app"` for this app's own `tracing` events, `"core"` for sing-box's
    /// child-process stdout/stderr lines.
    pub source: String,
    /// The `tracing` target/module path (e.g. `"core_manager"`), for
    /// `source: "app"` entries only -- always `None` for `"core"` lines,
    /// which have no equivalent concept.
    pub target: Option<String>,
    pub message: String,
}

/// Error type returned by every Tauri command (`Result<T, AppError>`).
/// Tauri serializes `Err` as the rejection value on the JS side, so the
/// frontend no longer needs the old `{success,data,error,code}` envelope —
/// `await invoke(...)` throws directly with this shape.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub code: String,
    pub message: String,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }
}

/// Outcome of probing one streaming/AI service through the currently
/// running proxy -- see `core_manager::unlock` for how each variant gets
/// decided. `Unknown` covers "we got a response but couldn't classify it"
/// (e.g. an unexpected HTTP status or a response shape that didn't parse),
/// distinct from `Error` (the request itself failed -- timeout, connection
/// refused, TLS failure, DNS failure through the proxy, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnlockStatus {
    Unlocked,
    Locked,
    Unknown,
    Error,
}

/// One entry of the `unlock_check` command's result list -- one per
/// built-in catalog service (see `core_manager::unlock::check_all`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockResult {
    /// Display name, e.g. `"Netflix"` -- not a stable machine id, since the
    /// catalog is small and built-in rather than user-editable.
    pub service: String,
    pub status: UnlockStatus,
    /// Detected region/country code, when the probe technique for this
    /// service can determine one (not every service's probe can -- see each
    /// `probe_*` function's doc comment in `core_manager::unlock`).
    pub region: Option<String>,
    /// Short human-readable extra context, e.g. `"Originals library only"`
    /// or an error message -- shown alongside the badge, not meant to be
    /// parsed.
    pub detail: Option<String>,
}

pub type AppResult<T> = Result<T, AppError>;
