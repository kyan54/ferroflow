//! Linux privileged helper: runs as a root systemd service, listens on a
//! Unix domain socket (`helper_proto::endpoints::LINUX_SOCKET`),
//! authenticates via `SO_PEERCRED` (kernel-verified peer UID) against a
//! root-owned allow-list (`LINUX_AUTHFILE`) instead of a shared token. On
//! `start`, forks and `setuid`s down to the calling user while granting
//! ambient capabilities (`CAP_NET_ADMIN`, `CAP_NET_RAW`,
//! `CAP_NET_BIND_SERVICE`) so sing-box runs unprivileged but can still
//! build a TUN device — capability lives on the process, not the binary,
//! so it survives core-binary swaps.
//!
//! STUB — being implemented by the helper-linux subagent. See
//! `docs/helper-design.md` and `helper_proto::Command` for the full
//! contract. Reference for behavior (not wire format): `helper-linux/`
//! (Go) in the Electron repo.
//!
//! Only meaningful on Linux; no-ops elsewhere so `cargo check --workspace`
//! passes from Windows/macOS dev machines.

#[cfg(target_os = "linux")]
mod service;

#[tokio::main]
async fn main() {
    #[cfg(target_os = "linux")]
    {
        tracing_subscriber::fmt::init();
        if let Err(err) = service::run().await {
            tracing::error!("helper-linux exited: {err}");
            std::process::exit(1);
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("helper-linux only runs on Linux");
        std::process::exit(1);
    }
}
