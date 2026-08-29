//! Filesystem / systemd-unit layout for the managed Linux install.
//!
//! Single source of truth shared by `service.rs` (the running root daemon)
//! and `install.rs` (the one-time pkexec install-script builder), so the two
//! can never drift apart. The socket path and allow-list path are *not*
//! duplicated here — they already live in `helper_proto::endpoints` as the
//! one thing every platform's client + helper must agree on byte-for-byte.
//!
//! Layout mirrors `LinuxServiceHelper.ts` / `helper-linux/helper.go` in the
//! Electron original (see module docs there for the FHS reasoning: installed
//! software lives under `/usr/local`, not `/opt`, to avoid colliding with a
//! future `.deb`'s package-managed tree).
//!
//! This whole module is only ever compiled in on Linux — see the `#[cfg]` on
//! its `mod paths;` declaration in `main.rs`, matching the existing
//! `mod service;` pattern, rather than gating from inside this file.

/// Root of the managed install: helper binary + managed core dir both live
/// under here. `rm -rf`-safe on uninstall (nothing else shares this path).
pub const INSTALL_DIR: &str = "/usr/local/lib/ferroflow";

/// Where the installer copies the helper binary itself.
pub const HELPER_DEST: &str = "/usr/local/lib/ferroflow/ferroflow-helper-linux";

/// Root-owned (root:root, 0755) managed core directory. Only `InstallCore`
/// (over the authenticated socket, sha256-verified) may write here, and
/// `Start` will only ever exec the binary at `CORE_BIN` — see the
/// core-path-lock check in `service.rs::cmd_start` for why this is the load-
/// bearing privilege-escalation guard of the whole crate.
pub const CORE_DIR: &str = "/usr/local/lib/ferroflow/core";

/// The one binary `Start` is ever allowed to exec.
pub const CORE_BIN: &str = "/usr/local/lib/ferroflow/core/sing-box";

pub const UNIT_NAME: &str = "ferroflow-helper.service";
pub const UNIT_PATH: &str = "/etc/systemd/system/ferroflow-helper.service";

/// Parent of `helper_proto::endpoints::LINUX_AUTHFILE`.
pub const STATE_DIR: &str = "/var/lib/ferroflow";

/// Parent of `helper_proto::endpoints::LINUX_SOCKET`. Also handed to
/// systemd as `RuntimeDirectory=` in the generated unit, so systemd creates
/// it (tmpfs, mode 0755) before `ExecStart` runs; `service.rs` additionally
/// creates it itself defensively in case the daemon is ever run outside
/// systemd (e.g. `--console`-style manual testing).
pub const RUNTIME_DIR: &str = "/run/ferroflow";
