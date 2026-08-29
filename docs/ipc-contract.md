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

`core-manager` and `net`'s methods currently return
`Err(AppError{code:"not_implemented",..})` — that's the seam the
core-manager/net/helper subagents fill in. The command layer itself doesn't
need to change when they do.

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
