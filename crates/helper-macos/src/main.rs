//! macOS privileged helper: runs as a root `launchd` LaunchDaemon, listens
//! on a Unix domain socket (`helper_proto::endpoints::MACOS_SOCKET`),
//! authenticates each request against a shared token file
//! (`MACOS_TOKEN_FILE`, mode 0600), and is the only thing on the box
//! allowed to start/stop the managed sing-box binary.
//!
//! Thin binary entry point — see `lib.rs` for the module layout, and
//! `service.rs`/`install.rs` for the implementation (install flow: one
//! `osascript ... with administrator privileges` prompt writes the plist +
//! bootstraps the daemon) and their unverified-on-real-macOS caveats. Full
//! command set in `helper_proto::Command`.
//!
//! Only meaningful when built for `*-apple-darwin`; on any other host this
//! is a no-op so `cargo check --workspace` still passes from Windows/Linux
//! dev machines.

#[tokio::main]
async fn main() {
    #[cfg(target_os = "macos")]
    {
        tracing_subscriber::fmt::init();
        if let Err(err) = helper_macos::service::run().await {
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
