//! Windows privileged helper: runs as a LocalSystem Windows Service
//! (`windows-service` crate), listens on a named pipe
//! (`helper_proto::endpoints::WINDOWS_PIPE`) with an ACL restricting
//! connections to SYSTEM + the interactive user (defense in depth on top
//! of the shared-token auth, same model as macOS).
//!
//! STUB — being implemented by the helper-windows subagent. See
//! `docs/helper-design.md` and `helper_proto::Command` for the full
//! contract. Reference for behavior (not wire format): `helper-win/`
//! (Go) in the Electron repo — service.go/winproc.go/selfuninstall.go.
//!
//! Also supports `--console` for foreground dev/test without installing
//! the service, matching the Go helper's dev workflow. No-ops on non-Windows
//! hosts so `cargo check --workspace` passes from macOS/Linux dev machines.

#[cfg(windows)]
mod service;

fn main() {
    tracing_subscriber::fmt::init();

    #[cfg(windows)]
    {
        let console_mode = std::env::args().any(|a| a == "--console");
        let result = if console_mode {
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            rt.block_on(service::run_console())
        } else {
            service::run_as_service()
        };
        if let Err(err) = result {
            tracing::error!("helper-windows exited: {err}");
            std::process::exit(1);
        }
    }

    #[cfg(not(windows))]
    {
        eprintln!("helper-windows only runs on Windows");
        std::process::exit(1);
    }
}
