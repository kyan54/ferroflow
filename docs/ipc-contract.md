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
| `servers_delete` | `id: string` | `UserConfig` | removes, clears `selectedServerId` if it matched |
| `rules_add` | `rule: RoutingRule` | `UserConfig` | appends, persists, returns full config |
| `rules_update` | `rule: RoutingRule` | `UserConfig` | replaces the rule with a matching `id`; no-op (current config unchanged) if the id isn't found |
| `rules_delete` | `id: string` | `UserConfig` | removes the rule with that id |
| `rules_reorder` | `orderedIds: string[]` | `UserConfig` | re-sorts `rules` to match `orderedIds`; ids not present in the running config are ignored, existing rules not named in `orderedIds` are appended after, keeping their relative order — call with the full current id list in a new order, not a partial reorder |
| `proxy_start` | `serverId: string` | `ProxyStatus` | looks up server, delegates to `core-manager` |
| `proxy_stop` | — | `ProxyStatus` | delegates to `core-manager` |
| `proxy_status` | — | `ProxyStatus` | delegates to `core-manager` |
| `system_proxy_status` | — | `SystemProxyStatus` | delegates to `net` |
| `platform_info` | — | `PlatformInfo` | `is_admin`/`os_version` still stubbed to `false`/`""` |
| `helper_get_status` | — | `HelperStatus` | pings the platform helper; `installed`/`ready` both `false` if unreachable |
| `helper_install` | — | `HelperStatus` | one-time elevated install (UAC/osascript/pkexec); see "Helper install flow" below |
| `helper_uninstall` | — | `HelperStatus` | reverses install |
| `subscription_import` | `url: string` | `UserConfig` | fetches + parses a subscription URL, appends the parsed servers, persists, returns full config; see "Subscription import" below |

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
(dev convenience, gitignored) → Tauri `resource_dir()` (packaged case, not
wired into `tauri.conf.json` bundling yet — that's a packaging-pass
follow-up, not blocking for dev/CI).

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
  options like `path`/`headerType`/`net` are parsed by no one and ignored,
  same MVP scope as `Protocol` itself (Vless/Trojan/Shadowsocks/Vmess only).
- A malformed or unsupported line is skipped, not fatal — the command only
  fails with `subscription_fetch_failed` (network/HTTP error) or
  `subscription_empty` (fetch succeeded but zero lines parsed as a server).

**Known limitation**: no dedupe. Importing the same subscription URL twice
appends duplicate servers rather than merging against what's already in
`UserConfig.servers` — there's no subscription-identity tracking (a
provider's URL, an update timestamp, ...) to dedupe against yet. Fine for a
one-shot "paste a URL, get servers" MVP flow; revisit once there's a UI for
managing/refreshing a named subscription rather than importing once.

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
  matchType: "domain" | "domainSuffix" | "domainKeyword" | "ipCidr" | "processName";
  values: string[];   // one or more raw match values for matchType
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
- **Scope**: domain (exact/suffix/keyword), IP CIDR, and process-name
  matching only — no GeoIP/GeoSite `.srs` rule-set file references. That's
  the separately-deferred "rule-resources" feature (see below), not this
  one.

## Types (`crates/shared-types/src/lib.rs`)

`Protocol` (Vless/Trojan/Shadowsocks/Vmess only for MVP — see file header),
`ServerConfig`, `ProxyMode`, `ProxyModeType`, `ProxyStatus`, `UserConfig`,
`RoutingRule`, `RuleMatchType`, `RuleOutbound`, `HelperStatus`,
`SystemProxyStatus`, `PlatformInfo`, `AppError`/`AppResult`. Field names are
`camelCase` on the wire (serde `rename_all`) to match the existing TS naming
convention.

## Deferred to phase 2 (do not build yet)

WARP/WireGuard/Tailscale, rule-resources (GeoIP/GeoSite `.srs` rule-set
*file* management/updates — distinct from the basic domain/IP/process
`RoutingRule` matching already implemented, see "Routing rules" above),
connection history, speed test, diagnostics/backup, sing-box dashboard
embedding, window-chrome commands (Linux custom titlebar), native file
dialogs. The Electron implementations of all of these are the reference for
*behavior* once we get to them — see `FlowZ/src/main/services/` in the
sibling Electron repo.
