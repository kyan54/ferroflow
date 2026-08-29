//! macOS privileged helper: runs as a root `launchd` LaunchDaemon, listens
//! on a Unix domain socket (`helper_proto::endpoints::MACOS_SOCKET`),
//! authenticates each request against a shared token file
//! (`MACOS_TOKEN_FILE`, mode 0600), and is the only thing on the box
//! allowed to start/stop the managed sing-box binary and touch routes/DNS.
//!
//! STUB — being implemented by the helper-macos subagent. See
//! `docs/helper-design.md` for the install flow (one `osascript ...  with
//! administrator privileges` prompt writes the plist + bootstraps the
//! daemon) and the full command set in `helper_proto::Command`.
//!
//! Only meaningful when built for `*-apple-darwin`; on any other host this
//! is a no-op so `cargo check --workspace` still passes from Windows/Linux
//! dev machines.

#[cfg(target_os = "macos")]
mod service;

#[tokio::main]
async fn main() {
    #[cfg(target_os = "macos")]
    {
        tracing_subscriber::fmt::init();
        if let Err(err) = service::run().await {
            tracing::error!("helper-macos exited: {err}");
            std::process::exit(1);
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("helper-macos only runs on macOS");
        std::process::exit(1);
    }
}
