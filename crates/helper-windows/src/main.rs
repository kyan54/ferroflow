//! Windows privileged helper: runs as a LocalSystem Windows Service
//! (`windows-service` crate), listens on a named pipe
//! (`helper_proto::endpoints::WINDOWS_PIPE`) with an ACL restricting
//! connections to SYSTEM + the interactive user (defense in depth on top
//! of the shared-token auth, same model as macOS).
//!
//! Reference for behavior (not wire format): `helper-win/` (Go) in the
//! Electron repo -- service.go/winproc.go/selfuninstall.go.
//!
//! Flags:
//!   --console    Run in the foreground without the SCM, for dev/test
//!                (matches the Go helper's dev workflow). No admin rights
//!                needed beyond whatever `--install` already set up.
//!   --install    One-time, admin-required setup: generate + ACL the
//!                shared token, register this exe as a LocalSystem
//!                service, and start it. See `install.rs`.
//!   --uninstall  Reverse of --install: stop + delete the service, remove
//!                the token and managed core dir. Also admin-required.
//!   (none)       Default: run as the registered service (this is what
//!                the SCM invokes on boot/start).
//!
//! No-ops on non-Windows hosts so `cargo check --workspace` passes from
//! macOS/Linux dev machines.

#[cfg(windows)]
mod install;
#[cfg(windows)]
mod pipe_acl;
#[cfg(windows)]
mod service;
#[cfg(windows)]
mod winproc;

fn main() {
    tracing_subscriber::fmt::init();

    #[cfg(windows)]
    {
        let args: Vec<String> = std::env::args().collect();
        let has = |flag: &str| args.iter().any(|a| a == flag);

        let result: anyhow::Result<()> = if has("--console") {
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            rt.block_on(service::run_console())
        } else if has("--install") {
            install::install()
        } else if has("--uninstall") {
            install::uninstall()
        } else {
            service::run_as_service()
        };

        if let Err(err) = result {
            tracing::error!("helper-windows exited: {err:#}");
            std::process::exit(1);
        }
    }

    #[cfg(not(windows))]
    {
        eprintln!("helper-windows only runs on Windows");
        std::process::exit(1);
    }
}
