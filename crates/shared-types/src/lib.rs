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
pub struct TlsConfig {
    pub enabled: bool,
    pub server_name: Option<String>,
    pub insecure: bool,
    pub reality_public_key: Option<String>,
    pub reality_short_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleOutbound {
    Proxy,
    Direct,
    Block,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub match_type: RuleMatchType,
    /// One or more raw match values (domains/suffixes/keywords/CIDRs/process
    /// names depending on `match_type`) -- no cross-field validation here,
    /// that's the UI's job to keep the input reasonable.
    pub values: Vec<String>,
    pub outbound: RuleOutbound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
            connection_history_enabled: false,
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
pub struct HelperStatus {
    pub platform: HelperPlatform,
    pub installed: bool,
    pub ready: bool,
    pub version: Option<String>,
    pub needs_repair: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemProxyStatus {
    pub enabled: bool,
    pub http_proxy: Option<String>,
    pub https_proxy: Option<String>,
    pub socks_proxy: Option<String>,
    pub bypass_list: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

pub type AppResult<T> = Result<T, AppError>;
