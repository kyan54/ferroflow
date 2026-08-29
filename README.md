# ferroflow

A Tauri 2 + Rust rewrite of [FlowZ](https://github.com/kyan54/FlowZ) (an
Electron sing-box proxy client), targeting Windows/macOS/Linux desktop with
a much smaller memory/CPU footprint than the Electron original.

**Status: functionally complete core proxy client, unsigned/unpackaged for
distribution.** All Rust crates and the frontend are verified in CI on all
three platforms on every push; see `docs/ipc-contract.md` for the exact
command surface and any per-feature caveats.

## What works today

- **Servers**: add/edit/delete (Vless, Trojan, Shadowsocks, Vmess, incl.
  Reality), or bulk-import from a subscription URL (base64 or plaintext
  share-link list).
- **Routing rules**: domain/domain-suffix/domain-keyword/IP-CIDR/process-name
  matching to proxy/direct/block, evaluated in list order.
- **Three take-over modes**: System Proxy (real OS-level proxy set/clear on
  all three platforms — Windows registry, macOS `networksetup`, Linux
  `gsettings`), Manual (local port, point your own apps at it), and TUN
  (via a privileged helper — see below).
- **Privileged helpers** (`helper-windows`/`helper-macos`/`helper-linux`):
  one-time elevated install (UAC/osascript/pkexec), zero-prompt privileged
  control after that, wired into the app with a Settings-page install/
  uninstall flow. `helper-windows` is exercised end-to-end on real Windows
  hardware; `helper-macos`/`helper-linux` compile and pass CI on real
  macOS/Linux runners but their *runtime* behavior (real launchd/systemd
  bootstrap, ambient capabilities) has not been exercised on real hardware
  by a human yet.
- **Live connections**: sing-box's built-in Clash API, polled every 2s —
  active connections, per-connection close, close-all, running traffic
  totals.
- **Backup/restore**: export/import the full config (servers, rules,
  settings) as a versioned JSON file via native save/open dialogs.
- **Diagnostic export**: a redacted Markdown report (secrets stripped) safe
  to paste into a bug report.
- **Packaging**: `npm run tauri build` produces a real MSI + NSIS installer
  on Windows (confirmed on this machine — both installers built, and the
  release binary launches standalone). macOS/Linux bundling is configured
  the same way but hasn't been run on real hardware; `.github/workflows/
  release.yml` builds all three platforms and drafts a GitHub Release on
  a `v*` tag push. **Nothing is code-signed or notarized** — same posture
  as the upstream Electron app (also unsigned).

## Known gaps

- **macOS/helper-linux runtime**: real hardware has never actually run
  `launchctl bootstrap`/`systemctl enable --now` for these helpers — CI
  only confirms they compile and pass unit tests on those targets.
- **No dedupe** on repeated subscription imports (append-only).
- **Clash API has no auth** (loopback-only, MVP simplification — see
  `docs/ipc-contract.md`).
- **Not implemented at all**: WARP/WireGuard/Tailscale mesh networking,
  persisted connection *history* (only live connections), speed test,
  the sing-box web dashboard embed, Linux custom titlebar chrome (the
  window currently uses the OS default decorations on all platforms).
- **No code signing** — Windows SmartScreen / macOS Gatekeeper will warn on
  install; the same is true of upstream FlowZ's own unsigned builds.

## Development

```bash
npm install
npm run tauri dev
```

Rust workspace: `cargo check --workspace` / `cargo test --workspace --exclude ferroflow`
(the `ferroflow` src-tauri package itself has no unit tests — the Tauri
app is exercised through `tauri dev`, not `cargo test`).

Build a real installer: `npm run tauri build` (Windows produces
`target/release/bundle/{msi,nsis}/`; needs the platform's native Tauri
prerequisites — see `.github/workflows/release.yml` for the exact package
list on Linux).

## Layout

```
src/                     React frontend
src-tauri/                Tauri app shell + command layer
crates/shared-types/       types shared across the whole Rust side
crates/helper-proto/       wire protocol shared by the three helpers
crates/helper-client/      unprivileged client for talking to the helpers
crates/core-manager/       sing-box process + config generation + Clash API client
crates/net/                system-proxy management (real, per-platform)
crates/subscription/       subscription-link fetch + parse
crates/helper-*/           per-platform privileged helper binaries
```
