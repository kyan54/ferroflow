//! Owns the sing-box child process lifecycle: config generation, start/stop
//! (via the platform helper when installed, falling back to a per-run
//! elevated spawn otherwise), and the gRPC status/connections stream
//! (`daemon.StartedService`, sing-box 1.14+).
//!
//! STUB — being implemented by the core-manager subagent. See
//! `docs/ipc-contract.md` for the Tauri command surface this crate backs.

pub mod config;
pub mod process;

use shared_types::{AppError, AppResult, ProxyStatus, ServerConfig};

pub struct CoreManager;

impl CoreManager {
    pub fn new() -> Self {
        Self
    }

    /// Returns a graceful `not_implemented` error rather than panicking, so
    /// the Tauri app + frontend are already exercisable end-to-end while
    /// this crate is being filled in — swap for the real implementation in
    /// place, callers don't need to change.
    pub async fn start(&self, _server: &ServerConfig) -> AppResult<ProxyStatus> {
        Err(AppError::new("not_implemented", "core-manager::start is not implemented yet"))
    }

    pub async fn stop(&self) -> AppResult<ProxyStatus> {
        Err(AppError::new("not_implemented", "core-manager::stop is not implemented yet"))
    }

    pub async fn status(&self) -> AppResult<ProxyStatus> {
        Ok(ProxyStatus::default())
    }
}

impl Default for CoreManager {
    fn default() -> Self {
        Self::new()
    }
}
