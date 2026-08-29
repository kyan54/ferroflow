# ferroflow

A Tauri 2 + Rust rewrite of [FlowZ](https://github.com/kyan54/FlowZ) (an
Electron sing-box proxy client), aimed at Windows/macOS/Linux desktop with
a much smaller memory/CPU footprint than the Electron original.

**Status: early MVP skeleton, not yet usable as a daily driver.** See
`docs/ipc-contract.md` for exactly what's implemented vs. deferred.

## What works today

- A Tauri app with a React frontend: add/edit/delete servers (Vless,
  Trojan, Shadowsocks, Vmess), start/stop a local sing-box proxy against
  the selected server, view its live status.
- `core-manager`: sing-box config generation + plain-process lifecycle,
  verified against a real sing-box binary.
- Three privileged-helper crates (`helper-windows`/`helper-macos`/
  `helper-linux`), each replicating FlowZ's "one elevation prompt, then
  zero-prompt privileged control" model for its platform, but **not yet
  wired into the app** — `core-manager` currently spawns sing-box as a
  plain unprivileged process, so TUN mode / privileged ports aren't
  available yet. `helper-windows` has been exercised end-to-end on a real
  Windows machine; `helper-macos`/`helper-linux` type-check against their
  real target triples but have only ever run in CI, not on real hardware.

## What's deferred (see `docs/ipc-contract.md`)

WARP/WireGuard/Tailscale, subscriptions, rule-sets, connection history,
speed test, diagnostics/backup, the sing-box dashboard, TUN mode, DNS
takeover, and wiring the app to the privileged helpers instead of spawning
sing-box directly.

## Development

```bash
npm install
npm run tauri dev
```

Rust workspace: `cargo check --workspace` / `cargo test --workspace --exclude ferroflow`
(the `ferroflow` src-tauri package itself has no unit tests — the Tauri
app is exercised through `tauri dev`, not `cargo test`).

## Layout

```
src/                  React frontend
src-tauri/            Tauri app shell + command layer
crates/shared-types/  types shared across the whole Rust side
crates/helper-proto/  wire protocol shared by the three helpers
crates/core-manager/  sing-box process + config generation
crates/net/           system-proxy management (stub)
crates/helper-*/      per-platform privileged helper binaries
```
