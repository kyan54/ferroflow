//! Cross-platform system-proxy management for the MVP (TUN/DNS-takeover is
//! phase 2 — see `SystemProxyManager.ts`/`SystemDnsManager.ts` in the
//! Electron app for the behavior this ports). Split per-OS since the
//! mechanism is unrelated on each platform: Windows registry
//! (`Internet Settings`), macOS `networksetup`, Linux `gsettings`.
//!
//! Each platform module owns a real `enable`/`disable`/`status` triplet
//! with this exact signature (see `windows.rs`/`macos.rs`/`linux.rs`);
//! this file only dispatches to whichever one matches the build target.

use shared_types::{AppResult, SystemProxyStatus};

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

    /// Points the OS-level system proxy at `127.0.0.1:<http_port>` for
    /// HTTP/HTTPS and `127.0.0.1:<socks_port>` for SOCKS. In this MVP,
    /// `core-manager` builds a single `mixed` sing-box inbound that serves
    /// both protocols on one port, so callers currently pass the same value
    /// for both — the two-parameter shape is kept anyway since OS-level
    /// proxy settings are inherently per-protocol (separate registry
    /// values / `networksetup`/`gsettings` keys) regardless of how many
    /// ports the backend actually listens on, so this won't need to change
    /// if that ever splits into separate inbounds.
    pub fn enable(&self, http_port: u16, socks_port: u16) -> AppResult<()> {
        #[cfg(target_os = "windows")]
        {
            windows::enable(http_port, socks_port)
        }
        #[cfg(target_os = "macos")]
        {
            macos::enable(http_port, socks_port)
        }
        #[cfg(target_os = "linux")]
        {
            linux::enable(http_port, socks_port)
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            let _ = (http_port, socks_port);
            Err(shared_types::AppError::new("unsupported_platform", "system proxy is not supported on this platform"))
        }
    }

    /// Idempotent: safe to call even if the system proxy isn't currently
    /// enabled (e.g. `CoreManager::stop`'s best-effort cleanup path).
    pub fn disable(&self) -> AppResult<()> {
        #[cfg(target_os = "windows")]
        {
            windows::disable()
        }
        #[cfg(target_os = "macos")]
        {
            macos::disable()
        }
        #[cfg(target_os = "linux")]
        {
            linux::disable()
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            Err(shared_types::AppError::new("unsupported_platform", "system proxy is not supported on this platform"))
        }
    }

    /// Reads the OS's actual current proxy configuration (not just "did we
    /// set it last") — this reflects reality even if the user or another
    /// app changed it since, or if the app restarted.
    pub fn status(&self) -> AppResult<SystemProxyStatus> {
        #[cfg(target_os = "windows")]
        {
            windows::status()
        }
        #[cfg(target_os = "macos")]
        {
            macos::status()
        }
        #[cfg(target_os = "linux")]
        {
            linux::status()
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            Ok(SystemProxyStatus::default())
        }
    }
}

impl Default for SystemProxyManager {
    fn default() -> Self {
        Self::new()
    }
}
