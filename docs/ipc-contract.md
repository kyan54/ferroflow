# IPC contract (MVP)

Frontend talks to the Rust backend exclusively through Tauri commands
(`@tauri-apps/api/core` `invoke()`), no custom events yet — the frontend
polls `proxy_status`/`config_get` for now. Push events (`proxy://started`,
`config://changed`, etc., mirroring `src/shared/ipc-channels.ts` in the
Electron app) are phase 2, once the MVP loop works.

All commands return `Result<T, AppError>` (see `crates/shared-types`).
Tauri serializes `Err` as the promise rejection, so the frontend just does:

```ts
try {
  const status = await invoke<ProxyStatus>('proxy_status');
} catch (err) {
  // err is AppError: { code: string, message: string }
}
```

## Commands implemented (`src-tauri/src/commands/`)

| Command | Args | Returns | Notes |
|---|---|---|---|
| `config_get` | — | `UserConfig` | reads in-memory state, loaded from `config.json` at startup |
| `config_save` | `config: UserConfig` | `()` | overwrites + persists |
| `servers_add` | `server: ServerConfig` | `UserConfig` | appends, persists, returns full config |
| `servers_update` | `server: ServerConfig` | `UserConfig` | replaces the server matching `server.id` in place (no-op if not found), preserving list position -- backs the Servers page's "Edit" button |
| `servers_delete` | `id: string` | `UserConfig` | removes, clears `selectedServerId` if it matched |
| `region_routing_update` | `regionRouting: RegionRoutingConfig` | `UserConfig` | replaces `region_routing` wholesale, persists, returns full config -- backs the Rules page's "地区分流" (region routing) card |
| `rules_add` | `rule: RoutingRule` | `UserConfig` | appends, persists, returns full config |
| `rules_update` | `rule: RoutingRule` | `UserConfig` | replaces the rule with a matching `id`; no-op (current config unchanged) if the id isn't found |
| `rules_delete` | `id: string` | `UserConfig` | removes the rule with that id |
| `rules_reorder` | `orderedIds: string[]` | `UserConfig` | re-sorts `rules` to match `orderedIds`; ids not present in the running config are ignored, existing rules not named in `orderedIds` are appended after, keeping their relative order — call with the full current id list in a new order, not a partial reorder |
| `rule_resources_catalog` | — | `CatalogEntry[]` | the curated built-in catalog (~20 entries) — see "Rule resources" below |
| `rule_resources_download` | `category: RuleResourceCategory, name: string` | `UserConfig` | downloads a catalog entry, stores it, upserts a `RuleResourceInfo` into `rule_resources`, persists; `rule_resource_not_in_catalog` if `name`/`category` don't match a catalog entry |
| `rule_resources_download_custom` | `name: string, category: RuleResourceCategory, url: string` | `UserConfig` | same, but for an arbitrary user-supplied name/URL not in the catalog (`isBuiltin: false`) |
| `rule_resources_update_all` | — | `UserConfig` | re-downloads every tracked resource using its original name/category and the *current* `githubAccelPrefix`; best-effort per resource, one failure doesn't abort the rest |
| `rule_resources_delete` | `id: string` | `UserConfig` | removes the file (best-effort) and the tracked entry; no-op on an unknown id |
| `proxy_start` | `serverId: string` | `ProxyStatus` | looks up server, delegates to `core-manager` |
| `proxy_stop` | — | `ProxyStatus` | delegates to `core-manager` |
| `proxy_status` | — | `ProxyStatus` | delegates to `core-manager` |
| `server_test_latency` | `serverId: string` | `number \| null` | raw TCP-connect timing (ms) to that server's `address:port`; `null` (not an error) on timeout/connect failure — see "Server latency test" below |
| `servers_test_latency_all` | — | `Record<string, number \| null>` | same probe, run concurrently across every server in the current config, keyed by server id |
| `connections_list` | — | `ConnectionsSnapshot` | delegates to `core-manager`'s Clash API client; `proxy_not_running` if nothing is running |
| `connections_close` | `id: string` | `()` | closes one connection by id |
| `connections_close_all` | — | `()` | closes every current connection |
| `dashboard_open` | — | `()` | opens (or focuses, if already open) the bundled sing-box dashboard window; `proxy_not_running` if the proxy isn't running; see "sing-box dashboard" below |
| `history_list` | — | `HistoryEntry[]` | reads the local connection-history log, most-recent-first; missing file (never enabled) returns `[]`, not an error; see "Connection history" below |
| `history_clear` | — | `()` | deletes the history file; idempotent, missing file is not an error |
| `logs_get` | — | `LogEntry[]` | reads the in-memory app+core log ring buffer, oldest-first; see "Logs" below |
| `logs_clear` | — | `()` | empties the log ring buffer |
| `system_proxy_status` | — | `SystemProxyStatus` | delegates to `net` |
| `platform_info` | — | `PlatformInfo` | `is_admin`/`os_version` still stubbed to `false`/`""` |
| `helper_get_status` | — | `HelperStatus` | pings the platform helper; `installed`/`ready` both `false` if unreachable |
| `helper_install` | — | `HelperStatus` | one-time elevated install (UAC/osascript/pkexec); see "Helper install flow" below |
| `helper_uninstall` | — | `HelperStatus` | reverses install |
| `subscription_import` | `url: string` | `UserConfig` | fetches + parses a subscription URL, appends the parsed servers, persists, returns full config; see "Subscription import" below |
| `subscription_import_text` | `text: string` | `UserConfig` | parses pasted, free-form share-link text (no network) through the same pipeline as `subscription_import`, appends, persists, returns full config |
| `subscription_import_file` | `path: string` | `UserConfig` | reads a local file (chosen by the frontend's native open dialog); `.yaml`/`.yml` is parsed as a Clash config's `proxies:` list, anything else as free-form share-link text; appends, persists, returns full config |
| `subscription_generate_share_url` | `server: ServerConfig` | `string` | builds the share-link for one server -- the exact inverse of `subscription::parse_uri`; pure/synchronous, no config mutation; `share_url_unsupported` for `Protocol::Wireguard`; see "Copy share link" below |
| `warp_register` | — | `UserConfig` | registers a new anonymous Cloudflare WARP device, appends it as a WireGuard server, persists, returns full config; see "Cloudflare WARP" below |
| `backup_export` | `path: string` | `()` | writes the current config as a versioned JSON backup to `path` (chosen by the frontend's native save dialog); see "Backup & diagnostics" below |
| `backup_import` | `path: string` | `UserConfig` | reads a versioned JSON backup from `path` (chosen by the frontend's native open dialog), replaces + persists the config, returns the new config |
| `diagnostic_export` | `path: string` | `()` | writes a redacted Markdown diagnostic report to `path`; see "Backup & diagnostics" below |
| `unlock_check` | — | `UnlockResult[]` | probes a built-in catalog of streaming/AI services through the current run's local proxy port; `proxy_not_running` if nothing is running or the current run has no local port (TUN mode); see "Streaming unlock status" below |

`net`'s methods currently return `Err(AppError{code:"not_implemented",..})`
— that's the seam a future `net` subagent fills in (system-proxy set/clear
isn't wired up yet, only its read-only `status()`).

`proxy_start`/`proxy_stop`/`proxy_status` now route through the platform
helper instead of a plain child process whenever
`config.proxyModeType === "tun"` **and** the helper is installed+ready;
otherwise (`"systemProxy"`/`"manual"`, or TUN requested with no helper)
they use the original unprivileged `core-manager`-owned child process.
Requesting TUN with no helper installed returns
`AppError{code:"helper_unavailable"}` (`ProxyErrorCode::HelperUnavailable`)
rather than silently falling back — the frontend should call
`helper_get_status`/`helper_install` first and only then retry `proxy_start`.

## Helper install flow

One-time, one-elevation-prompt setup, mirroring each helper crate's own
`install.rs` doc comments (see `crates/helper-{windows,macos,linux}/src/install.rs`):

- **Windows**: `commands::helper_windows` generates a token, writes it to a
  private temp file, runs the bundled `ferroflow-helper-windows.exe
  --install --token-file <path>` via `Start-Process -Verb RunAs` (one UAC
  prompt), then persists the token client-side (see below) and deletes the
  temp file.
- **macOS**: `commands::helper_macos` calls
  `helper_macos::install::build_install_script(helper_binary_path)`
  (already generates + returns the token — no separate generation step),
  writes the returned script to a private temp file, runs it via
  `osascript -e 'do shell script "/bin/bash <path>" with administrator
  privileges'` (one prompt), persists the token, deletes the temp file.
- **Linux**: `commands::helper_linux` calls
  `helper_linux::install::build_install_script(helper_binary_path, uid,
  bundled_core_path)`, writes it to a temp file, runs `pkexec /bin/sh
  <path>` (one prompt). No token — Linux auth is `SO_PEERCRED`-based.

All three then poll `HelperClient::ping()` (bounded retries — systemd/
launchd/SCM need a moment to actually start the unit) before returning
`HelperStatus{installed:true, ready:true, ..}`.

Token persistence (Windows/macOS only): `<app_config_dir>/helper-token`,
loaded into `AppState.helper_token` at startup and pushed into
`CoreManager::set_helper_token` — see `state.rs`. Binary discovery for the
bundled helper executable mirrors `core-manager`'s `locate_binary`
convention: `FERROFLOW_HELPER_PATH` env var → `.dev-bin/<helper-exe-name>`
(dev convenience, gitignored) → `<resource_dir>/helper/<helper-exe-name>`
(packaged case — staged there by `npm run build:helper`, which
`bundle.resources` maps to `helper/` inside the bundle; see
`scripts/build-helper.mjs`). The sing-box *core* binary itself follows the
identical three-tier pattern (`FERROFLOW_SINGBOX_PATH` → `.dev-bin/sing-box`
→ `<resource_dir>/singbox/`, staged by `npm run fetch:singbox`/
`scripts/fetch-singbox.mjs`) — see `CoreManager::locate_binary_with_resource_dir`
and `state::init_binary_path`. TUN-mode starts additionally push this same
binary (hashed) to the privileged helper via the `InstallCore` pipe command
before every `Start`, since the helper only ever runs its own
install-time-verified managed copy, never a path the app hands it directly
(security fix noted on the `Start` command below) — see `CoreManager::start`'s
`Tun` arm.

## Subscription import

`commands::subscription::subscription_import` (backed by the `subscription`
crate) fetches a provider's subscription URL and bulk-imports every server
it contains, rather than requiring one server to be typed in by hand at a
time. Supported input shapes, mirroring what real-world proxy providers
actually distribute:

- **Body encoding**: either the whole response body base64-encoded, or
  plain text — one share-link per line. `subscription::decode_subscription_body`
  detects which by attempting a base64 decode and checking whether the
  result contains a recognizable share-link scheme; if not, the body is
  assumed to already be plaintext.
- **Share-link formats**: `vless://`, `trojan://`, `ss://` (both the SIP002
  base64-userinfo form and a plain unencoded `method:password@host:port`
  fallback), and `vmess://` (base64-encoded JSON). Only the fields
  `core-manager`'s outbound builder actually consumes are extracted (`uuid`/
  `flow`, `password`, `encryption`, TLS/Reality settings) — transport
  options like `path`/`headerType`/`net` are parsed by no one and ignored.
  **WireGuard has no parser here** — there is no standardized WireGuard
  share-link URI scheme in wide use the way there is for the other four
  protocols, so WireGuard servers can only be added by hand via `ServerForm`
  (see "WireGuard" under "Types" below).
- A malformed or unsupported line is skipped, not fatal — the command only
  fails with `subscription_fetch_failed` (network/HTTP error) or
  `subscription_empty` (fetch succeeded but zero lines parsed as a server).

**Known limitation**: no dedupe. Importing the same subscription URL twice
appends duplicate servers rather than merging against what's already in
`UserConfig.servers` — there's no subscription-identity tracking (a
provider's URL, an update timestamp, ...) to dedupe against yet. Fine for a
one-shot "paste a URL, get servers" MVP flow; revisit once there's a UI for
managing/refreshing a named subscription rather than importing once.

Every server produced by any of the three entry points below (URL, paste, or
file) gets `source: "subscription"` (see "Server source" under "Types"
below); a server built by hand via `ServerForm`, or by `warp_register`, gets
`source: "manual"`.

**Three import entry points, one shared tail.** The Servers page's import
modal (`ServersView.tsx`'s `SubscriptionImportForm`) offers three modes,
each backed by its own command but all funneling into the same
append-persist-return tail (`commands::subscription::import_servers`):

- **URL** (`subscription_import`) — as above.
- **Paste text** (`subscription_import_text`) — a multi-line textarea of raw
  share-links, no network fetch; reuses `subscription::parse_subscription_body`
  directly on the pasted string (which already tolerates a whole-body
  base64-wrapped block, same as the URL path's fetched body).
- **File** (`subscription_import_file`) — the frontend resolves a path via
  `@tauri-apps/plugin-dialog`'s `open()` (filtered to `.txt`/`.yaml`/`.yml`)
  and passes it straight through; the command reads the file itself and
  branches on extension: `.yaml`/`.yml` (case-insensitive) goes through
  `subscription::parse_clash_yaml` (see "Clash YAML import" below), anything
  else goes through the same `parse_subscription_body` path as pasted text.

All three fail with `subscription_empty` if parsing yields zero servers, and
`subscription_import_file` additionally fails with
`subscription_file_read_failed` if the path can't be read at all (missing
file, permissions, ...).

## Clash YAML import

`subscription::parse_clash_yaml` (`crates/subscription/src/clash.rs`) is a
second, pure/side-effect-free parser alongside `parse.rs`'s share-link
parser, for the other common real-world subscription shape: a Clash-style
YAML config. It extracts the top-level `proxies:` sequence and converts each
entry to a `ServerConfig`, covering the same four protocols this app
actually supports end to end — `vless`, `trojan`, `ss` (shadowsocks), and
`vmess` (matching `Protocol`'s variants minus `wireguard`, which has no
share-link *or* Clash-YAML convention to target). Any other `type` (e.g.
`hysteria2`, `ssr`, `snell`) is skipped, counted but not fatal — same
per-entry-skip policy as a malformed share-link line.

Parsing is deliberately lenient (`serde_yaml::Value` field-by-field
extraction, not one strict `#[derive(Deserialize)]` struct) because
real-world Clash configs vary in key naming across generators — e.g. both
`reality-opts`/`reality_opts` and both nested `public-key`/`public_key` are
accepted, and `skip-cert-verify` or `insecure` either one sets
`TlsConfig.insecure`. Only the fields `core-manager`'s outbound builder
actually reads are extracted (same scope note as the share-link parser) —
transport options (`network`, `ws-opts`, `grpc-opts`, ...) are ignored. An
unparseable document (not valid YAML, or no top-level `proxies` sequence)
yields an empty result rather than an error; `subscription_import_file` is
what turns an empty result into the user-facing `subscription_empty` error.

## Cloudflare WARP

`commands::warp::warp_register` (backed by the `warp` crate) is a one-click
alternative to hand-entering a WireGuard server: it registers a brand-new,
free, **anonymous** device with Cloudflare's real WARP registration API
(`api.cloudflareclient.com`), then maps the response directly onto a
WireGuard `ServerConfig` and appends it (named "Cloudflare WARP", with a
`" (2)"`/`" (3)"`/... suffix if that name is already taken) exactly like
`servers_add`/`subscription_import` do. No form, no user input — the button
just registers and appends.

This is **not** reverse-engineering an undocumented private API: it's the
same public, unauthenticated self-service registration endpoint the
well-known open-source `wgcf` project (and the official WARP mobile apps)
use to mint anonymous WireGuard identities. Any caller can `POST` a freshly
generated X25519 public key and get back a working WireGuard peer config —
no Cloudflare account, login, or API token involved.

Two fixed-value choices worth knowing about, both confirmed against real,
successful registrations (see `crates/warp`'s doc comment and its ignored
live integration test):

- Cloudflare's response embeds the peer endpoint as `<ip>:0` — a
  placeholder port, not a usable one. `warp_register` strips it and pairs
  the bare IP with a **fixed** port of `2408` (WARP's well-known primary UDP
  port, and `wgcf`'s own default) rather than trusting that placeholder or
  resolving the response's `endpoint.host` at runtime.
- The response's (unmodeled) `policy.tunnel_protocol` field may say
  `"masque"`, Cloudflare's newer default hint. This is irrelevant here:
  sing-box has no MASQUE support, and the classic WireGuard fields this app
  reads (`config.peers[0]`, `config.interface`) remain fully valid
  regardless of that hint — again, exactly what `wgcf` relies on.

**Known limitation (deliberate scope cut, not an oversight)**: deleting the
resulting server via `servers_delete` does **not** deregister the device
from Cloudflare's side. `crates/warp` does expose a `deregister()` function
(and this app's own dev workflow uses it to clean up test registrations),
but `servers_delete` isn't wired to call it in this pass — there's no
`device_id`/`token` tracking on `ServerConfig` to key that call off of, and
an orphaned anonymous WARP registration costs Cloudflare nothing and carries
no account attached to it, so this isn't treated as urgent. A future pass
could add those two fields to `ServerConfig` (WireGuard-only, like the
existing `wireguard_*` fields) if cleaning up abandoned registrations turns
out to matter.

## Copy share link / Clone to self-built

Two per-server actions on the Servers page card (`ServersView.tsx`'s
`ServerCard`), mirroring buttons in the sibling Electron app's node list
(`server-actions.tsx`'s `copyShareUrl`/`cloneToManual`):

- **Copy share link** — calls `subscription_generate_share_url` (backed by
  `subscription::generate_share_url`, the exact inverse of
  `subscription::parse_uri`) and copies the result to the clipboard via
  `navigator.clipboard.writeText`, same mechanism `LogsView`'s "Copy all"
  already uses. Hidden for `protocol === "wireguard"` (`hasShareLink` in
  `ServersView.tsx`) rather than shown disabled, since WireGuard has no
  share-link format at all — see "WireGuard" below.
- **Clone to self-built** — only shown for `source === "subscription"`
  servers (see "Server source" below). Functionally identical to the
  always-present "Duplicate" button (new id, appended via `servers_add`)
  except the clone is force-set to `source: "manual"` and gets a distinct
  name suffix, so it becomes an independent copy no future re-import of the
  same subscription can be confused with. Store-side: `cloneToSelfBuilt` in
  `store.ts`, right next to `duplicateServer`.

### Server source (`ServerSource`, manual vs. subscription)

The sibling Electron app distinguishes a node's origin via `subscriptionId`
(`string | undefined`) — a pointer back to a persisted, refreshable
subscription entity, letting it cascade-delete a subscription's nodes or
warn that editing one will be overwritten on next sync. ferroflow's
subscription import (see "Subscription import" above) has no such entity —
it's a one-shot fetch-parse-append with no dedupe and nothing to refresh
against (see that section's "Known limitation") — so `ServerConfig.source`
is deliberately a lighter two-value enum (`"manual" | "subscription"`)
rather than a copy of `subscriptionId`: it can gate "Clone to self-built"'s
visibility, but **cannot** support a refresh/cascade-delete the real app's
field does, since there's nothing on this side to refresh from. `#[serde(default)]`
(→ `"manual"`) so a `config.json` persisted before this field existed still
loads. Set by:

- `ServerForm`'s manual-add path and `warp_register` → `"manual"`.
- All four protocol parsers in `subscription::parse` and all four Clash-YAML
  converters in `subscription::clash` (i.e. every `subscription_import*`
  command) → `"subscription"`.
- `duplicateServer` (existing "Duplicate" button) preserves whatever the
  original had; `cloneToSelfBuilt` always overrides to `"manual"`.

## Backup & diagnostics

`commands::backup` (`src-tauri/src/commands/backup.rs`) adds config
backup/restore and a redacted diagnostic export, all writing to/reading from
a `path` the **frontend** picks via `@tauri-apps/plugin-dialog`'s
`save()`/`open()` -- these commands never show a dialog themselves, they just
do the file I/O they're told to.

**Backup envelope.** `backup_export` does not write a bare `UserConfig` --
it wraps it in a small versioned envelope so a future incompatible format
change can be detected on import instead of silently misinterpreted:

```json
{
  "ferroflowBackupVersion": 1,
  "config": { /* UserConfig, same camelCase shape as config_get/config_save */ }
}
```

`backup_import` checks `ferroflowBackupVersion` first: anything other than
exactly `1` fails with `backup_incompatible` (no guess-migration -- that's a
future problem once there's a second version to migrate from). A malformed
file (bad JSON, missing `ferroflowBackupVersion`/`config` fields, or a
`config` that doesn't match `UserConfig`'s shape) fails with
`backup_invalid`, never a panic. On success, the imported config replaces
`AppState.config`, is persisted to `config.json` the same way `config_save`
does, and is returned to the frontend so the UI updates immediately.

**Diagnostic export redaction.** `diagnostic_export` writes a Markdown
report meant to be pasted directly into a GitHub issue. Exactly what's
redacted:

- **Servers**: `name`, `protocol`, `address`, `port`, `tls.enabled`, and
  `tls.serverName` (SNI) are included -- these are what's actually useful
  for diagnosing a connection problem. `uuid`, `password`,
  `tls.realityPublicKey`, and `tls.realityShortId` are **never written to
  the file at all** (not masked in place -- simply omitted from the table).
- **Rules**: included in full (name, enabled, match type, values, outbound)
  -- a routing rule's domains/IPs/process names are the entire point of the
  rule, not a secret.
- **Settings**: the non-secret `UserConfig` fields (`proxyMode`,
  `proxyModeType`, `autoStart`, `silentStart`, `autoConnect`,
  `minimizeToTray`, `language`, `selectedServerId`) are included in full.
- Also included: app version (`env!("CARGO_PKG_VERSION")`), `PlatformInfo`
  (via `commands::system::platform_info`), current `ProxyStatus` (via
  `core_manager.status()`), current `SystemProxyStatus` (via
  `system_proxy.status()`), and `HelperStatus` (via
  `commands::helper::helper_get_status`, reused rather than duplicated --
  its section reads "unavailable" instead of failing the whole export if
  that check errors).

## Routing rules

`UserConfig.rules: RoutingRule[]` lets a user route selected traffic
differently from the "everything through the proxy" default, instead of the
previous all-or-nothing behavior — e.g. "domain suffix `.cn` goes direct" or
"this IP range is blocked". Each `RoutingRule` is:

```ts
{
  id: string;
  name: string;
  enabled: boolean;
  matchType: "domain" | "domainSuffix" | "domainKeyword" | "ipCidr" | "processName" | "ruleSet";
  values: string[];   // raw match values for matchType, EXCEPT "ruleSet" -- see "Rule resources" below
  outbound: "proxy" | "direct" | "block";
}
```

- **One match field per rule.** Each rule sets exactly one of the five
  match types against its `values` — there's no support for compound
  conditions (e.g. "this domain AND this process") in this pass.
- **Evaluation order.** `core_manager::config::build_route_rules` maps
  enabled rules with at least one value into a sing-box `route.rules` array,
  preserving list order. sing-box evaluates `route.rules` top to bottom and
  stops at the first match; `route.final` (already present, pointed at the
  proxy outbound) is the fallback when no rule matches. Reordering rules in
  the UI (`rules_reorder`) therefore changes routing behavior, not just
  display order — put more specific rules above more general ones.
- **Disabled/empty rules are skipped**, not written into the config at all
  — toggling a rule off is equivalent to deleting it from sing-box's point
  of view, without losing its definition in `UserConfig`.
- **`block` outbound.** A `block`-type sing-box outbound is only added to
  the generated config when at least one enabled rule actually references
  it, to avoid emitting an outbound nothing points at.
- **Scope**: domain (exact/suffix/keyword), IP CIDR, process-name matching,
  and GeoIP/GeoSite `.srs` rule-set references (`matchType: "ruleSet"`) --
  see "Rule resources" below for that last one.
- **Fallback outbound.** `UserConfig.defaultOutbound` (`"proxy"` by default,
  same as this app's behavior before the field existed) is the tag
  sing-box's `route.final` resolves to -- what happens to traffic that
  matches no enabled rule. `core_manager::config::build_config_with_inbound`
  also promotes a `block` outbound into existence when `defaultOutbound` is
  `"block"`, same as it already does for a `block`-outbound rule. Driven by
  region presets (see "App routing & region presets" below), which need
  "proxy only these rule-sets, everything else direct" -- not expressible
  via `route.rules` alone, since sing-box has no literal "match everything"
  rule condition.

## Rule resources

GeoIP/GeoSite `.srs` rule-set files (sing-box's binary rule-set format) that
a `RoutingRule` can reference by name (`matchType: "ruleSet"`) instead of
typing thousands of domains/IPs by hand — e.g. "everything in
`geosite-netflix.srs` goes through the proxy". Backed by the standalone
`rule-resources` crate (catalog + URL-building + download mechanics, no
knowledge of `UserConfig`/sing-box config shape) plus a thin
`src-tauri/src/commands/rule_resources.rs` state/storage layer on top.

**Download source.** SagerNet (sing-box's author) publishes one small,
individual `.srs` file per category on a dedicated `rule-set` branch of each
of its `sing-geosite`/`sing-geoip` repos (not a GitHub release):

```
https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-<name>.srs
https://raw.githubusercontent.com/SagerNet/sing-geoip/rule-set/geoip-<name>.srs
```

Each branch holds well over a thousand files — `rule_resources::builtin_catalog()`
is a small, curated set of ~25 commonly useful ones (`cn`/`geolocation-!cn`
GeoSite, `cn`/`private` GeoIP, and per-service GeoSite entries like
`netflix`/`youtube`/`google`/`github`/`openai`/`telegram`/`tiktok`/
`disney`/`spotify`/`twitter`/`facebook`/`instagram`/`microsoft`/`apple`/
`amazon`/`category-ads-all`/`discord`/`steam`/`playstation`/`anthropic`/
`docker`), not an exhaustive list. Any other valid upstream filename can
still be added via `rule_resources_download_custom`, which takes an
arbitrary name/URL directly.

**GitHub-acceleration prefix.** `UserConfig.githubAccelPrefix` (optional,
`None`/blank by default) is an opaque string prepended verbatim in front of
the real `raw.githubusercontent.com` URL — e.g. a "GitHub 加速" mirror like
`https://ghproxy.com/`. `rule_resources::resource_url` does no validation or
special-casing of the prefix; it's plain string concatenation, matching a
pattern common in Chinese networking tools for working around
GitHub-blocked-in-China network conditions.

**Storage.** Downloaded to `<app_config_dir>/rule-resources/<category>-<name>.srs`
(`state::rule_resources_dir`), atomically (`rule_resources::download` writes
to `<path>.tmp` then renames over the real path). Each download's size and
SHA-256 are recorded on the corresponding `RuleResourceInfo` in
`UserConfig.rule_resources`, keyed by `id` -- `"<category file-prefix>-<name>"`
(e.g. `"geosite-netflix"`, `"geoip-cn"`; see `commands::rule_resources::resource_id`),
**not** bare `name` -- the catalog deliberately has entries that share a
`name` across both categories (`"cn"` is both a GeoSite and a GeoIP entry),
so an id derived from `name` alone would collide between them.

**Referencing a resource from a rule.** `RuleMatchType::RuleSet`
(`matchType: "ruleSet"` on the wire) is the one `RoutingRule.match_type`
variant where `values` holds `RuleResourceInfo.id`s instead of literal
domains/IPs — `RuleForm`'s values editor becomes a checkbox list of
`config.ruleResources` for this match type, so a rule can only reference
resources that have actually been downloaded.

**How it maps into the generated sing-box config.**
`core_manager::config::build_route_rules`/`build_rule_set_entries` turn this
into sing-box's real `route.rule_set` local-file feature:

```json
{
  "route": {
    "rule_set": [
      { "type": "local", "tag": "ruleset-netflix", "format": "binary", "path": "<absolute path to the .srs file>" }
    ],
    "rules": [
      { "rule_set": ["ruleset-netflix"], "outbound": "proxy" }
    ],
    "final": "proxy"
  }
}
```

One `route.rule_set` entry is emitted per **distinct** resource id actually
referenced by an *enabled* `RuleSet` rule with a known downloaded path — an
id with no known path (never downloaded, or deleted since the rule was
created) is skipped with a `tracing::warn!`, and if every id in a rule
resolves to nothing, the whole rule is skipped, never a panic or a broken
config. `CoreManager::start` takes the `id -> .srs path` map as a
`resource_paths: &HashMap<String, PathBuf>` parameter, built by
`commands::proxy::proxy_start` from `UserConfig.rule_resources` plus the
`rule_resources_dir` storage convention.

**Auto update.** `UserConfig.ruleResourceAutoUpdate` (opt-in, off by
default, mirrors `connectionHistoryEnabled`'s convention) +
`ruleResourceAutoUpdateIntervalHours` (default 24) control a standalone
background task (`commands::rule_resources::spawn_auto_update_task`,
`JoinHandle`-based like `core_manager::history::HistoryRecorder` but *not*
tied to `proxy_start`/`proxy_stop` — rule-resource freshness has nothing to
do with whether a proxy run is active). Started once from `lib.rs`'s
`.setup()` hook and left running for the app's whole lifetime; it re-reads
both settings fresh on every wake-up, so toggling them takes effect on the
next tick without a restart.

## App routing & region presets

`AppRoutingView` (`src/views/AppRoutingView.tsx`, `src/lib/appRouting.ts`) is
a friendlier UI layer on top of the exact same `RoutingRule`/rule-resources
infra above -- **no new Tauri commands**, it only calls `rules_add`/
`rules_update`/`rules_delete`, `rule_resources_download`, and `config_save`,
same as `RulesView`/`RuleResourcesView`/`SettingsView` already do.

**App routing.** `APP_ROUTING_CATALOG` is a curated ~17-entry list of
well-known apps/services grouped into categories (Streaming, Social, AI
tools, Gaming, Dev & productivity), each mapped to one existing GeoSite
catalog entry. Each app gets a 4-way control (Off/Proxy/Direct/Block); the
page derives its current value by looking for a `RoutingRule` whose `id` is
`` `app-routing:${appId}` `` (see `appRoutingRuleId`) in `config.rules` --
this deterministic id convention is the "marker" that lets the page read its
own state back out of the plain rules list on load, instead of needing a
dedicated field on `RoutingRule`. Setting a value other than "Off" downloads
the backing GeoSite resource first if `config.ruleResources` doesn't already
have it, then adds (or updates, if the rule already exists) a
`matchType: "ruleSet"` rule referencing it; setting "Off" deletes the rule
if present.

**Region presets.** `REGION_PRESETS` is a small fixed set (China direct/rest
proxy, streaming via proxy/rest direct, block ads + China direct/rest proxy,
global proxy with no rules). Each preset bundles one or more GeoSite/GeoIP
resource ids into a `RoutingRule` (sing-box's `rule_set` field matches if
traffic matches *any* of the listed rule-sets, so e.g. GeoSite `cn` + GeoIP
`cn` in one rule covers "China by domain or by IP") and sets
`UserConfig.defaultOutbound` -- see "Routing rules" above for why a preset
needs to touch that field (expressing "proxy only X, everything else Y" isn't
possible with `route.rules` alone). Applying a preset:

1. Downloads any referenced resource not already in `config.ruleResources`.
2. Removes only *that preset's own* previously-applied rules -- ids prefixed
   `preset:<presetId>:` (see `PRESET_RULE_PREFIX`) -- leaving manual rules and
   `AppRoutingView` toggles in `config.rules` untouched. The one exception is
   "Global proxy, no rules" (`clearsAllRules: true`), which is explicitly a
   wipe-everything preset by design and clears the whole `rules` array.
3. Appends the preset's fresh rules and saves via `config_save`.

The frontend arms a preset button on first click ("Apply" -> "Confirm?") and
applies it on the second, mirroring the two-step confirm pattern already
used for destructive actions elsewhere (`RulesView`/`ServersView`/
`RuleResourcesView` delete buttons, `SettingsView`'s "Remove helper") rather
than a modal dialog.

## Live connections

Every sing-box run now enables sing-box's own built-in Clash API
(`experimental.clash_api.external_controller`, a well-documented, stable
sing-box feature) on a second `127.0.0.1` port picked the same way as the
local proxy inbound's port (`CoreManager::pick_local_port`) — in both
`SystemProxy`/`Manual` mode (alongside the `mixed` inbound) and `Tun` mode
(there's no local inbound to piggyback on there, but traffic visibility
shouldn't depend on which inbound is active). `core_manager::clash_api` is a
thin `reqwest` client for the three endpoints this app uses:

- `GET /connections` → `ConnectionsSnapshot { downloadTotal, uploadTotal,
  connections: ConnectionInfo[] }`
- `DELETE /connections/{id}` → closes one connection
- `DELETE /connections` → closes all of them

`connections_list`/`connections_close`/`connections_close_all` delegate
straight through to `CoreManager::list_connections`/`close_connection`/
`close_all_connections`, which look up the current run's Clash API port and
fail with `AppError{code:"proxy_not_running"}` if nothing is running, or
`AppError{code:"clash_api_error"}` if the HTTP call itself fails. The
frontend (`ConnectionsView.tsx`) polls `connections_list` every 2 seconds,
same pattern as `DashboardView`'s `proxy_status` polling, and treats a
`proxy_not_running` failure as "show an empty table" rather than a toast —
that's the expected, common state whenever the proxy isn't running.

**No auth.** The Clash API is enabled with no `secret` configured and bound
to `127.0.0.1` only — any local process can query or close connections on
that port while the proxy is running. This is a deliberate MVP
simplification (loopback-only, no remote exposure), not an oversight;
revisit if the local port is ever considered a meaningful attack surface
(e.g. add a per-run random `secret` and pass it as a Bearer token once
there's a reason to).

**Totals are cumulative-since-start.** `downloadTotal`/`uploadTotal` (and
each connection's own `upload`/`download`) are sing-box's own running
totals since the process started — this app doesn't compute, reset, or
window them itself. Stopping and restarting the proxy resets them to zero
along with everything else, since it's a fresh sing-box process each time.

## Connection topology

`ConnectionTopology.tsx` (Dashboard's "连接拓扑"/"Connection topology" Sankey
diagram) is frontend-only -- **no new Tauri commands**. It re-shapes the
same `connectionsSnapshot` `DashboardView` already polls (see "Live
connections" above) via `src/lib/connectionTopology.ts`
(device -> host -> exit-node aggregation, by connection *count* not bytes)
and `src/lib/sankeyLayout.ts` (coordinates), same pattern as "App routing &
region presets" reusing existing infra instead of adding a command.

**Data source: live snapshot only, deliberately.** This was ported from the
reference FlowZ Electron app's own "连接拓扑" page (found in the sibling
`FlowZ` checkout, `src/renderer/components/home/` +
`src/main/services/connections-aggregate.ts`) rather than approximated --
and that app's own topology is *also* built only from currently-open
connections, never from persisted history: its `StatsService` deletes a
connection from its tracking map the instant sing-box reports it `CLOSED`.
A diagram with 15+ distinct named hosts doesn't require this app's opt-in
"connection history" feature (see "Connection history" below) at all; it's
just what a real browsing session's concurrently-open connections
(keep-alives, background sync, polling, etc.) looks like at any one moment.
Consequently the topology card shows the same two empty states as
`ConnectionsView`'s active-connections table (not running / no active
connections) regardless of whether `connectionHistoryEnabled` is on.

**Host cutoff.** The reference app folds hosts into an "其他"/"Other" bucket
past its own `TOPOLOGY_TOP_N = 15` (not "top 8") -- `connectionTopology.ts`
matches that constant exactly.

**Add-rule context menu.** Right-clicking a named host node opens a small
menu (built from scratch -- ferroflow had no dropdown/context-menu
primitive, and the reference app's is a much larger feature this port
intentionally doesn't replicate in full; see that file's doc comment) with
three one-click actions (Proxy/Direct/Block) that build a `RoutingRule`
(`domainSuffix` for a resolved host, `ipCidr` for a bare destination IP,
suffixed `/32` or `/128`) and call the existing `rules_add` — same command
`RuleForm`/`RulesView` already use, not a dedicated one. A minimal
exact-value duplicate check (not the reference app's full
domainSuffix-subsumes-subdomain "already covered by an earlier rule"
analysis) avoids piling up repeat rules from right-clicking the same host
twice.

## sing-box dashboard

`commands::dashboard::dashboard_open` opens SagerNet/sing-box-dashboard's
official web UI in a second Tauri window, pointed at the same Clash API
described in "Live connections" above — this is sing-box's own upstream
monitoring/connections/logs dashboard, offered alongside (not instead of)
this app's simpler built-in Connections tab. Mirrors the sibling Electron
app, which bundles the same dashboard's built assets and opens them in a
second `BrowserWindow`.

**Fetched, not committed.** The dashboard's static build (the `gh-pages`
branch of that repo — that branch *is* the Vite build output) is downloaded
by `scripts/fetch-dashboard.mjs` (`npm run fetch:dashboard`) into
`src-tauri/resources/dashboard/`, which is gitignored (large, third-party,
reproducible from the script — same reasoning as `/.dev-bin/`) and listed
under `bundle.resources` in `tauri.conf.json` so it ships in a packaged
build. `dashboard_open` locates `index.html` via the same three-tier
discovery convention as `commands::helper_windows`'s bundled-helper-binary
lookup: `FERROFLOW_DASHBOARD_PATH` env var (pointing at the directory, not
the file) → `<src-tauri crate dir>/resources/dashboard/` (dev convenience,
anchored via `CARGO_MANIFEST_DIR` rather than the process's current working
directory, since this asset specifically lives under `src-tauri/` regardless
of where the app happens to be launched from) → Tauri's `resource_dir()`
(packaged case). Missing assets (i.e. `fetch:dashboard` was never run) fail
with `AppError{code:"dashboard_missing"}` rather than opening a blank/broken
window.

**Requires the proxy running first.** `dashboard_open` fails with
`AppError{code:"proxy_not_running"}` if `core_manager::current_clash_api_port()`
returns `None` — there is no Clash API port to point the dashboard at
otherwise. The frontend button (`DashboardView`'s "Open sing-box dashboard",
next to the Start/Stop controls) is disabled whenever `running` is `false`,
same gating as those controls.

**Connection-info mechanism (read from the fork's real fetched source, not
assumed).** This fork of the dashboard takes **no URL query parameters at
all** — its bundle (`assets/index-*.js`) has zero references to
`URLSearchParams`/`location.search` anywhere; this was confirmed by
downloading the actual `gh-pages` zip and grepping the minified bundle
directly, since this detail varies per Clash-dashboard fork (Yacd/Razord/
metacubexd all differ from this one). Its server list instead lives in
`localStorage` under the key `sing-box-dashboard.servers`, JSON-shaped as
`{ servers: { id, name, url, secret }[], activeId: string }`. (There is also
a legacy singular `sing-box-dashboard.server` key — `{ url, secret }` — that
the app reads and deletes exactly once, purely to migrate an old
single-server install into the list format; `dashboard_open` deliberately
does **not** rely on that path, since it only fires when `servers` has never
been set at all, which would mean a second dashboard open silently kept
pointing at a *previous* run's now-stale port — `clash_api_port` is a fresh
ephemeral port every `core_manager` `start()` call.)

`dashboard_open` instead seeds the modern `servers` key directly, via
`WebviewWindowBuilder::initialization_script` (runs before any of the page's
own scripts, on every navigation). The injected script reads whatever is
already stored, upserts one entry keyed by the fixed id `ferroflow-local`
with this run's `http://127.0.0.1:<clash_api_port>` and an empty `secret`
(matching "No auth" above), preserves any other servers the user added by
hand, and always sets `activeId` to `ferroflow-local` — so the dashboard
connects to *this* run's Clash API immediately on open, every time, rather
than requiring a manual pick or risking a stale entry. See
`src-tauri/src/commands/dashboard.rs`'s module doc comment for the full
byte-for-byte reasoning and the exact script template.

**Known limitation: the current gh-pages build talks gRPC-Web, not the
classic Clash REST API — verified against a real sing-box, not assumed.**
Everything above is about getting the *address* of the local Clash API to
the dashboard correctly, and that part is verified working (the injected
`{id, name, url, secret}` entry is read back exactly as written — confirmed
by opening the real fetched `index.html` and inspecting its "Edit server"
dialog). But the dashboard's own connectivity layer
(`src/api/daemon.ts` in the dashboard's source repo) exclusively speaks
**gRPC-Web (Connect-RPC) to a `daemon.StartedService`** at that URL — there
is no REST fallback in the current build at all, including for the basic
overview/status view. This service is **not** one of sing-box's documented
`experimental.clash_api` fields (checked against the live
sing-box.sagernet.org docs: `external_controller`, `external_ui`, `secret`,
`default_mode`, `access_control_allow_origin`, etc. — no `daemon`/gRPC
option among them), and it is not served by either a stable `sing-box run`
(tested against 1.13.19) or the newest available prerelease at the time of
writing (1.14.0-rc.2) — both were started locally with `clash_api` enabled
on a known port and both produced the dashboard's own
"Failed to fetch" → "connection failed" state when pointed at that port,
the same failure mode you'd see if the address were wrong. (Its
`useDiagnosedConnectError`/`diagnoseConnection` heuristic then further
mislabels this as "CORS blocked" specifically, since its own no-cors probe
against *any* reachable port succeeds regardless of whether the right
service is actually listening on it — sing-box's Clash API does in fact
send `Access-Control-Allow-Origin: *`, confirmed via `curl`, so CORS itself
is not the real problem here.)

Net effect: **as of this writing, opening the dashboard will reliably show
a "connection failed" state**, not because `dashboard_open`'s wiring is
wrong, but because the upstream dashboard's rolling `gh-pages` build has
moved to a next-generation sing-box daemon API ahead of any public sing-box
release actually serving it. This mirrors, and is presumably the same
underlying feature as, `core_manager`'s own longstanding note (see this
crate's module doc comment) that "the gRPC status/connections stream
(`daemon.StartedService`, sing-box 1.14+) is still a later pass" — i.e. this
app doesn't implement that service either. Revisit `fetch-dashboard.mjs`
once sing-box ships a stable, documented way to enable
`daemon.StartedService` over HTTP (at which point `core_manager` would also
need to start serving it, not just `clash_api`), or consider pinning an
older dashboard commit/tag that still speaks the classic REST API if this
gap persists and a working dashboard is wanted sooner.

## Streaming unlock status

`unlock_check` (no args) probes a small built-in catalog of streaming/AI
services -- Netflix, Disney+, YouTube Premium, ChatGPT, Spotify, Prime Video
-- through the currently running server, and returns one `UnlockResult` per
service:

```ts
interface UnlockResult {
  service: string;           // display name, e.g. "Netflix"
  status: "unlocked" | "locked" | "unknown" | "error";
  region?: string | null;    // detected region/country code, when determinable
  detail?: string | null;    // short human-readable extra context
}
```

The actual probing lives in `core_manager::unlock` (`check_all`), not in the
Tauri command layer (`commands::unlock::unlock_check` is thin delegation to
`CoreManager::check_unlock`). Each service issues one or two unauthenticated
HTTP requests to that provider's public-facing edge, through a `reqwest`
client configured with an HTTP proxy pointed at the current run's local
`mixed` inbound port (`ProxyStatus.local_port` -- the same port
`SystemProxy`/`Manual` mode's local proxy inbound uses; `reqwest` talking to
it as an HTTP proxy is enough to tunnel HTTPS via CONNECT too), then
classifies the response. All six probes run concurrently
(`tokio::join!`), each under an 8-second timeout.

**Requires a local proxy port.** `check_unlock` fails with
`AppError{code:"proxy_not_running"}` if nothing is running, or if the
current run has no local inbound to probe through at all (`Tun` mode has no
`local_port` -- see that field's doc comment). The frontend
(`UnlockStatusCard`, on the Dashboard) shows this as an inline message
rather than a toast and disables the "Check unlock status" button whenever
the proxy isn't running.

**Manually triggered, not polled.** Unlike `proxy_status`/`connections_list`,
the frontend never calls this on an interval -- each call makes real
outbound requests to several external services and can take a few seconds,
so it only runs when the user clicks the button.

**Best-effort by nature, same as any such tool.** Each probe's technique
(a specific Netflix title id returning 404 vs 200, a JSON field in Disney+'s
public geo API, a substring in YouTube's Premium page, OpenAI's
`unsupported_country` 403 response, a locale-prefixed redirect from
Spotify's signup page, Prime Video's redirect-to-`amazon.com` behavior) is
the same class of technique community "media unlock checker" scripts use,
and carries the same maintenance burden: a provider changing a title's
licensing, an error page's copy, or a JSON schema can silently degrade a
probe to `UnlockStatus::Unknown` (response received but not classifiable)
or `UnlockStatus::Error` (request itself failed). See
`crates/core-manager/src/unlock.rs`'s module doc comment and each
`probe_*` function for the exact technique and its known limitations.

## Server latency test

`commands::latency` (`server_test_latency`/`servers_test_latency_all`) gives
the Servers page a quick per-node ping/latency indicator, using the same
lightweight technique real-world proxy-client UIs use: **a raw TCP connect
to the server's own `address:port`, wall-clock timed** — not a full proxy
handshake through sing-box, and not routed through the local proxy port the
way `unlock_check`'s probes are. This deliberately means it works with the
proxy fully stopped (no sing-box process, no privileged helper needed at
all) — it's testing "can I reach this endpoint and how long does the TCP
handshake take", the same signal a ping/traceroute-style indicator in any
proxy client conveys, not "does the full protocol handshake succeed".

- `server_test_latency(serverId)` looks up the server the same way
  `proxy_start` does (`AppError{code:"server_not_found"}` if the id doesn't
  match), then times one `tokio::net::TcpStream::connect` against
  `server.address:server.port`, capped at a 3-second `tokio::time::timeout`.
  Returns `Ok(Some(ms))` on a successful connect, **`Ok(None)`** (not an
  `Err`) on a timeout or any connect failure (DNS failure, connection
  refused, host unreachable, ...) — a server not responding is an expected,
  legitimate probe result the UI shows inline (e.g. "Timeout"), not an error
  toast.
- `servers_test_latency_all()` runs the same probe concurrently across every
  server in the current config via a `tokio::task::JoinSet` (a dynamic-
  length analogue of `core_manager::unlock::check_all`'s fixed-arity
  `tokio::join!` fan-out, since the server list's length varies), returning
  one `serverId -> Option<ms>` entry per server. Like the single-server
  variant, an individual server's probe failing surfaces as that entry's
  `None`, never a batch-wide `Err`.
- The frontend's "Test all" button (`ServersView`) calls the batch command
  and merges the result into `latencyResults` (`store.ts`); each card's own
  lightning-bolt button calls the single-server command for just that node.
  Neither call is polled — same "only runs when the user asks" convention as
  `unlock_check`.

## Connection history

Distinct from "Live connections" above: instead of showing what's happening
*right now*, this is an **opt-in, off-by-default** local log of connections
that have already finished, so a user can look back at what happened after
the fact. Mirrors the sibling Electron app's own `ConnectionHistoryService`
in spirit (opt-in, no source IP/request content logged).

**Opt-in.** `UserConfig.connectionHistoryEnabled` defaults to `false`. When
`false` (the default), nothing is ever written — no background task is even
spawned. Turning it on only takes effect for the *next* time the proxy
starts; flipping it while the proxy is already running does not retroactively
start logging that session (there's no live-reconfiguration path for this,
deliberately — matching this codebase's stated preference for not
over-engineering an MVP feature). The Settings toggle ("Record connection
history", in the Behavior section) says this explicitly so the UI never
implies recording started immediately.

**What's recorded.** `core_manager::history::HistoryRecorder` runs a
background task (spawned by `CoreManager::start` alongside the sing-box
process, when enabled) that polls `list_connections` every 5 seconds and
diffs consecutive snapshots by connection id: any connection present in the
previous poll but gone from the current one is "finished" and gets appended
as one `HistoryEntry` — same shape as the live `ConnectionInfo` (destination
metadata, upload/download byte counts, outbound chain, matched rule) plus an
`end` timestamp (RFC3339, generated by this app the moment the connection is
noticed gone — sing-box itself has no "closed at" timestamp). A connection
that both starts and finishes inside a single 5-second poll window is never
observed as "present" and so is invisible to history — an accepted gap for a
best-effort look-back log, not a complete audit trail.

**Storage: local, unencrypted, capped.** Written to
`<app_config_dir>/connection-history.jsonl` as plain JSON, one `HistoryEntry`
object per line, in append order (oldest first on disk; `history_list`
reverses it to most-recent-first for display). **There is no encryption** —
if this is turned on on a shared machine, anyone with filesystem access to
that directory can read every logged destination. The file is capped at the
**1000 most recent** entries: any write that would exceed that truncates the
oldest lines first, so it never grows unbounded.

**Lifecycle.** The recorder is aborted (`JoinHandle::abort()`, not a
cooperative cancellation channel — see `CoreManager::stop_running`'s doc
comment for why a hard abort is an acceptable simplification here) whenever
the tracked sing-box run stops, whether via an explicit `proxy_stop` or a new
`proxy_start` superseding it, so a stale recorder never keeps appending to
the file after its run has ended.

## Logs

A live, in-memory diagnostic log view -- distinct from "Connection history"
above (which only ever records finished *connections*, not general app/core
activity) and always-on (no opt-in toggle, since it logs no destination/
traffic data, just operational messages). Mirrors the sibling Electron app's
Logs page.

**What's captured.** `core_manager::logs::LogBuffer` is a single bounded
`VecDeque<LogEntry>` (capped at `MAX_LOG_LINES` = 2000, oldest evicted first)
fed from two sources at once:

- **This app's own `tracing` events** (`source: "app"`) -- `src-tauri`'s
  `log_layer::LogCaptureLayer`, a custom `tracing_subscriber::Layer` installed
  alongside (not instead of) the existing stdout `fmt` layer in `run()`, so
  every `tracing::info!`/`warn!`/`error!`/etc. call anywhere in the backend is
  captured with no per-call-site changes needed. `target` is the `tracing`
  target/module path (e.g. `"core_manager"`).
- **sing-box's own stdout/stderr** (`source: "core"`, `target: null`) --
  `CoreManager::start` (`Backend::Local` only; `ProcessHandle::take_stdio`)
  spawns one `core_manager::logs::spawn_line_reader` task per stream right
  after a successful spawn, reading line-by-line and pushing each non-blank
  line in. `level` is a best-effort guess from known sing-box level tokens
  (`FATAL`/`ERROR`/`WARN`/`DEBUG`/`TRACE`, defaulting to `Info`) -- sing-box's
  raw text output isn't structurally parsed. These reader tasks are aborted
  in `stop_running` alongside `history_task`, same reasoning (best-effort
  background forwarders, no critical section). Not wired up for
  `Backend::Helper` (`Tun` mode) -- the helper owns that child process, not
  `core-manager` directly; out of scope for this pass.

**In-memory only, no opt-in, no persistence.** Unlike connection history,
nothing is ever written to disk and there is no `UserConfig` toggle -- the
buffer just accumulates for the app's running lifetime and is cleared on
`logs_clear` or app restart. `LogEntry` shape:

```ts
{
  timestamp: string;        // RFC3339, when this entry was captured
  level: "trace" | "debug" | "info" | "warn" | "error";
  source: string;           // "app" | "core"
  target?: string | null;   // tracing target/module path, "app" entries only
  message: string;
}
```

`logs_get` returns the buffer oldest-first (a live log view reads top to
bottom, unlike `history_list`'s most-recent-first look-back convention).
`logs_clear` empties it. The frontend (`LogsView.tsx`) polls `logs_get` every
2 seconds, same interval/cleanup pattern as `ConnectionsView`'s
`connections_list` polling, and auto-scrolls to the bottom on new entries.

## Types (`crates/shared-types/src/lib.rs`)

`Protocol` (Vless/Trojan/Shadowsocks/Vmess/Wireguard for MVP — see file
header), `ServerConfig`, `ServerSource` (see "Server source" above),
`ProxyMode`, `ProxyModeType`, `ProxyStatus`,
`UserConfig`, `RoutingRule`, `RuleMatchType`, `RuleOutbound`,
`RuleResourceCategory`, `RuleResourceInfo`, `HelperStatus`,
`SystemProxyStatus`, `PlatformInfo`, `ConnectionMetadata`, `ConnectionInfo`,
`ConnectionsSnapshot`, `HistoryEntry`, `LogLevel`, `LogEntry`, `UnlockStatus`,
`UnlockResult`, `AppError`/`AppResult`. Field names are `camelCase`
on the wire (serde `rename_all`) to match the existing TS naming convention
— with one deliberate exception: `ConnectionMetadata::destination_ip` is
explicitly renamed to `destinationIP` (not the `camelCase`-derived
`destinationIp`) to match sing-box's own Clash API wire field exactly, since
a mismatch there would silently deserialize to an empty string rather than
fail to compile.

### WireGuard

WireGuard servers are **manual-entry-only**: unlike Vless/Trojan/
Shadowsocks/Vmess, there is no standardized WireGuard share-link URI scheme
in wide use, so `subscription::parse` has no WireGuard parser and never
will until one becomes a de-facto standard worth targeting. A WireGuard
`ServerConfig` carries four dedicated fields instead of the usual
`uuid`/`password`/`encryption`/`flow`: `wireguardPrivateKey`,
`wireguardPeerPublicKey`, `wireguardPreSharedKey` (optional), and
`wireguardLocalAddress` (single CIDR address, e.g. `10.0.0.2/32`) — the
peer endpoint itself reuses the existing `address`/`port` fields. WireGuard
also has **no TLS layer** (`tls` is always `null` for it — it's a distinct
crypto handshake, not something TLS wraps); `ServerForm` hides the TLS
fieldset entirely when `protocol === "wireguard"`.

On the `core-manager` side, note that sing-box removed the `wireguard`
*outbound* type in 1.13.0 (deprecated since 1.11.0) in favor of a
WireGuard *endpoint* — `core-manager::config::build_outbound`'s
`Wireguard` arm builds that endpoint shape, and
`build_config_with_inbound` places it under the generated config's
top-level `endpoints` array rather than `outbounds`. It's still tagged and
referenced exactly like every other protocol's proxy outbound (`route.final`
and `RuleOutbound::Proxy` both resolve to the same `PROXY_OUTBOUND_TAG`),
so this is purely an internal JSON-shape detail invisible to the IPC
surface.

## Deferred to phase 2 (do not build yet)

Tailscale (WireGuard itself and one-click Cloudflare WARP registration are
now implemented — see "WireGuard" and "Cloudflare WARP" above), speed test,
window-chrome commands (Linux custom titlebar). Rule resources (GeoIP/GeoSite
`.srs` rule-set file management/updates) are now implemented — see "Rule
resources" above. Config
backup/restore, a redacted diagnostic export, native file dialogs, persisted
connection history (opt-in, distinct from the live, in-memory connection
list — see "Live connections" and "Connection history" above), and the
sing-box dashboard (see "sing-box dashboard" above) are implemented. The
Electron implementations of
the still-deferred items are the reference for *behavior* once we get to
them — see `FlowZ/src/main/services/` in the sibling Electron repo.
