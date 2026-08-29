# ferroflow

A Tauri 2 + Rust rewrite of [FlowZ](https://github.com/kyan54/FlowZ) (an
Electron sing-box proxy client), targeting Windows/macOS/Linux desktop with
a much smaller memory/CPU footprint than the Electron original.

**Status: functionally complete core proxy client, unsigned for
distribution.** All Rust crates and the frontend are verified in CI on all
three platforms on every push; see `docs/ipc-contract.md` for the exact
command surface and any per-feature caveats.

## What works today

- **Servers**: add/edit/delete (Vless, Trojan, Shadowsocks, Vmess, WireGuard,
  incl. Reality), or bulk-import from a subscription URL (base64 or
  plaintext share-link list).
- **Cloudflare WARP**: one-click anonymous device registration against
  Cloudflare's real public API (the same one `wgcf` uses), producing a
  ready-to-use WireGuard server.
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
- **Connection history** (opt-in, off by default): a local, unencrypted
  JSON-lines log of finished connections, capped at 1000 entries.
- **sing-box official dashboard**: opens SagerNet's real dashboard build in
  a second window, pre-seeded with the running instance's connection info.
  Wired correctly (verified byte-for-byte against the real fetched source),
  but currently can't show live data against any released sing-box version
  — see "Known gaps".
- **Backup/restore**: export/import the full config (servers, rules,
  settings) as a versioned JSON file via native save/open dialogs.
- **Diagnostic export**: a redacted Markdown report (secrets stripped) safe
  to paste into a bug report.
- **Packaging**: `npm run tauri build` produces a real MSI + NSIS installer
  on Windows, confirmed twice on this machine (including with the bundled
  dashboard assets — installers grew from ~9MB/5.7MB to ~16MB/12.6MB as
  expected). macOS/Linux bundling is configured the same way but hasn't
  been run on real hardware. `.github/workflows/release.yml` builds all
  three platforms and drafts a GitHub Release on a `v*` tag push — the
  full pipeline (including GitHub Release creation, which needed an
  explicit `contents: write` permission fix) has been validated end to end.
  **Nothing is code-signed or notarized** — same posture as the upstream
  Electron app.

## Known gaps

- **macOS/helper-linux runtime**: real hardware has never actually run
  `launchctl bootstrap`/`systemctl enable --now` for these helpers — CI
  only confirms they compile and pass unit tests on those targets.
- **No dedupe** on repeated subscription imports (append-only). Deleting a
  WARP-derived server locally doesn't deregister the device from Cloudflare.
- **Clash API has no auth** (loopback-only, MVP simplification — see
  `docs/ipc-contract.md`).
- **sing-box dashboard shows "connection failed"**: the real, current
  `SagerNet/sing-box-dashboard` gh-pages build talks exclusively gRPC-Web to
  a `daemon.StartedService` RPC that no released sing-box version (1.13.19
  stable or 1.14.0-rc.2, both tested) actually serves. This app's side of
  the integration is correct; it'll start working once sing-box ships that
  service in a release.
- **Not implemented at all**: Tailscale (sing-box has no native Tailscale
  outbound the way it does for WireGuard; the upstream Electron app embeds
  Go's `tsnet`, which has no drop-in Rust equivalent — would need either
  shelling out to a real `tailscale` CLI the user already has installed, or
  a much larger custom integration), speed test.
- **No code signing** — Windows SmartScreen / macOS Gatekeeper will warn on
  install; the same is true of upstream FlowZ's own unsigned builds. This
  needs a real certificate the project doesn't have, not just more code.
- **Window chrome**: uses each OS's native title bar/decorations
  everywhere (not a gap vs. the upstream Electron app's custom-drawn
  frame — native decorations need no per-desktop-environment upkeep and
  are the simpler, more robust default for a Tauri app).

## Development

```bash
npm install
npm run fetch:dashboard   # required -- see note below, even for cargo check
npm run tauri dev
```

**`npm run fetch:dashboard` is not optional**, even if you don't care about
the dashboard feature: Tauri's build script validates every path in
`tauri.conf.json`'s `bundle.resources` at *compile* time, not just when
actually bundling — `cargo check`/`cargo build`/`cargo test` on the
`ferroflow` package will fail with `resource path "resources\dashboard"
doesn't exist` until this has been run once. (This bit CI initially; both
`build.yml` and `release.yml` run it before any `cargo` step.)

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
crates/warp/                Cloudflare WARP registration
crates/helper-*/           per-platform privileged helper binaries
```
