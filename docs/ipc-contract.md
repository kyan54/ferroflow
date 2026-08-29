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
| `proxy_start` | `serverId: string` | `ProxyStatus` | looks up server, delegates to `core-manager` |
| `proxy_stop` | — | `ProxyStatus` | delegates to `core-manager` |
| `proxy_status` | — | `ProxyStatus` | delegates to `core-manager` |
| `system_proxy_status` | — | `SystemProxyStatus` | delegates to `net` |
| `platform_info` | — | `PlatformInfo` | `is_admin`/`os_version` still stubbed to `false`/`""` |
| `helper_get_status` | — | `HelperStatus` | pings the platform helper; `installed`/`ready` both `false` if unreachable |
| `helper_install` | — | `HelperStatus` | one-time elevated install (UAC/osascript/pkexec); see "Helper install flow" below |
| `helper_uninstall` | — | `HelperStatus` | reverses install |

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

## Types (`crates/shared-types/src/lib.rs`)

`Protocol` (Vless/Trojan/Shadowsocks/Vmess only for MVP — see file header),
`ServerConfig`, `ProxyMode`, `ProxyModeType`, `ProxyStatus`, `UserConfig`,
`HelperStatus`, `SystemProxyStatus`, `PlatformInfo`, `AppError`/`AppResult`.
Field names are `camelCase` on the wire (serde `rename_all`) to match the
existing TS naming convention.

## Deferred to phase 2 (do not build yet)

WARP/WireGuard/Tailscale, subscriptions, rule-resources, connection
history, speed test, diagnostics/backup, sing-box dashboard embedding,
window-chrome commands (Linux custom titlebar), native file dialogs. The
Electron implementations of all of these are the reference for *behavior*
once we get to them — see `FlowZ/src/main/services/` in the sibling
Electron repo.
