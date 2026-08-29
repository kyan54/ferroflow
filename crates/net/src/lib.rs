//! Cross-platform system-proxy management for the MVP (TUN/DNS-takeover is
//! phase 2 — see `SystemProxyManager.ts`/`SystemDnsManager.ts` in the
//! Electron app for the behavior this ports). Split per-OS since the
//! mechanism is unrelated on each platform: Windows registry
//! (`Internet Settings`), macOS `networksetup`, Linux `gsettings`/env.

use shared_types::{AppError, AppResult, SystemProxyStatus};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod linux;

pub struct SystemProxyManager;

impl SystemProxyManager {
    pub fn new() -> Self {
        Self
    }

    pub fn enable(&self, _http_port: u16, _socks_port: u16) -> AppResult<()> {
        Err(AppError::new("not_implemented", "net::SystemProxyManager::enable is not implemented yet"))
    }

    pub fn disable(&self) -> AppResult<()> {
        Err(AppError::new("not_implemented", "net::SystemProxyManager::disable is not implemented yet"))
    }

    pub fn status(&self) -> AppResult<SystemProxyStatus> {
        Ok(SystemProxyStatus::default())
    }
}

impl Default for SystemProxyManager {
    fn default() -> Self {
        Self::new()
    }
}
