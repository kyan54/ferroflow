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
//! Implemented against `helper_proto::Command` (see that crate for the full
//! wire contract). Reference for *behavior* (not wire format):
//! `helper-linux/` (Go) in the Electron repo.
//!
//! Only meaningful on Linux; no-ops elsewhere so `cargo check --workspace`
//! passes from Windows/macOS dev machines.
//!
//! Developed on a Windows machine with no Linux available. It does compile
//! (full codegen, not just typeck — `cargo check`/`cargo build
//! --target x86_64-unknown-linux-gnu` both get through rustc cleanly, and
//! `cargo clippy` is silent; only the final link step fails here, for lack
//! of a cross-linker, which is a host tooling gap, not a code issue) and it
//! type-checks against the real `nix`/`caps`/`tokio` APIs it calls (verified
//! against docs.rs / upstream source during review, not from memory), but it
//! has never *run*: no fork/setuid/capability sequence in this crate has
//! executed against a real kernel. That happens for the first time in CI on
//! an actual Linux runner. See the implementation report for exactly which
//! syscall-ordering assumptions to scrutinize first.
//!
//! Thin binary entry point — see `lib.rs` for the module layout (mirrors
//! `helper-macos`'s bin/lib split, so `install`/`paths` are callable from
//! `src-tauri` as a library too, not just from this binary).

#[tokio::main]
async fn main() {
    #[cfg(target_os = "linux")]
    {
        tracing_subscriber::fmt::init();
        if let Err(err) = helper_linux::service::run().await {
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
