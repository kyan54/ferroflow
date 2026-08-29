//! Windows system-proxy via `HKCU\...\Internet Settings` + `InternetSetOption`
//! broadcast (`INTERNET_OPTION_SETTINGS_CHANGED`/`INTERNET_OPTION_REFRESH`)
//! so running apps (Edge/IE-based) pick up the change without a reboot.
//!
//! Registry access goes through `winreg` (much simpler/less error-prone than
//! raw FFI for this part); the `InternetSetOptionW` broadcast is the one
//! piece that genuinely needs `windows-sys`, since there's no safe wrapper
//! for it in this workspace's dependency set.

use std::io;

use shared_types::{AppError, AppResult, SystemProxyStatus};
use windows_sys::Win32::Networking::WinInet::{
    InternetSetOptionW, INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED,
};
use winreg::enums::*;
use winreg::RegKey;

/// `HKEY_CURRENT_USER`-relative path -- per-user setting, same key the
/// classic Windows "Internet Options" control panel / Settings > Network >
/// Proxy page reads and writes, and the one every legacy IE-based /
/// WinINet-based app consults (Edge included, for anything that still
/// shells out to WinINet rather than its own network stack).
const INTERNET_SETTINGS_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

/// `<local>` is Windows' special `ProxyOverride` token meaning "bypass the
/// proxy for any address that doesn't contain a dot" (plain hostnames --
/// typically LAN/intranet machines). Included by default so intranet-style
/// hostnames aren't forced through the proxy, matching what real proxy
/// clients set out of the box.
const DEFAULT_PROXY_OVERRIDE: &str = "<local>;127.0.0.1;localhost";

pub(crate) fn enable(http_port: u16, socks_port: u16) -> AppResult<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _disposition) = hkcu
        .create_subkey(INTERNET_SETTINGS_PATH)
        .map_err(|e| proxy_error("failed to open/create Internet Settings registry key", e))?;

    // Windows' legacy IE-style `ProxyServer` value needs the `socks=` prefix
    // spelled out explicitly, or SOCKS traffic won't route through it at all
    // -- a bare `host:port` only ever covers HTTP/HTTPS.
    let proxy_server = format!(
        "http=127.0.0.1:{http_port};https=127.0.0.1:{http_port};socks=127.0.0.1:{socks_port}"
    );

    key.set_value("ProxyServer", &proxy_server)
        .map_err(|e| proxy_error("failed to write ProxyServer", e))?;
    key.set_value("ProxyOverride", &DEFAULT_PROXY_OVERRIDE)
        .map_err(|e| proxy_error("failed to write ProxyOverride", e))?;
    key.set_value("ProxyEnable", &1u32)
        .map_err(|e| proxy_error("failed to write ProxyEnable", e))?;

    broadcast_settings_changed()
}

/// Idempotent by design: a missing key/values (fresh machine, proxy never
/// enabled) or an already-0 `ProxyEnable` are both treated as success, not
/// an error -- this is called from `CoreManager`'s best-effort cleanup paths
/// (explicit `stop()`, and "detected the process died on its own"), which
/// must never fail loudly just because there was nothing to clean up.
///
/// `ProxyServer`/`ProxyOverride` are deliberately left in place -- they're
/// harmless while `ProxyEnable` is 0, and keeping them means the next
/// `enable()` doesn't need to re-derive an override list from scratch.
pub(crate) fn disable() -> AppResult<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.open_subkey_with_flags(INTERNET_SETTINGS_PATH, KEY_SET_VALUE) {
        Ok(key) => {
            key.set_value("ProxyEnable", &0u32)
                .map_err(|e| proxy_error("failed to write ProxyEnable", e))?;
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            // Key doesn't exist at all -- nothing was ever enabled.
            return Ok(());
        }
        Err(e) => return Err(proxy_error("failed to open Internet Settings registry key", e)),
    }

    broadcast_settings_changed()
}

/// Reads the OS's actual current proxy configuration, not just "did we set
/// it last" -- reflects reality even if the user or another app changed it
/// since, or the app restarted. A missing key or missing values is treated
/// as "not enabled" (fresh machine / never touched), not an error.
pub(crate) fn status() -> AppResult<SystemProxyStatus> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = match hkcu.open_subkey(INTERNET_SETTINGS_PATH) {
        Ok(key) => key,
        Err(_) => return Ok(SystemProxyStatus::default()),
    };

    let proxy_enable: u32 = key.get_value("ProxyEnable").unwrap_or(0);
    let proxy_server: String = key.get_value("ProxyServer").unwrap_or_default();
    let proxy_override: String = key.get_value("ProxyOverride").unwrap_or_default();

    let (http_proxy, https_proxy, socks_proxy) = parse_proxy_server(&proxy_server);

    let bypass_list: Vec<String> = proxy_override
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    Ok(SystemProxyStatus {
        enabled: proxy_enable == 1 && !proxy_server.is_empty(),
        http_proxy,
        https_proxy,
        socks_proxy,
        bypass_list,
    })
}

/// Parses the legacy IE-style `ProxyServer` value. It's either:
/// - protocol-specific: `http=host:port;https=host:port;socks=host:port`
///   (any subset -- sing-box's `mixed` inbound is what we write, but
///   something else may have set only one or two), or
/// - a bare `host:port` with no `=` at all, which historically means "use
///   this for HTTP/HTTPS" (and implicitly nothing for SOCKS).
fn parse_proxy_server(raw: &str) -> (Option<String>, Option<String>, Option<String>) {
    if raw.is_empty() {
        return (None, None, None);
    }

    if !raw.contains('=') {
        return (Some(raw.to_string()), None, None);
    }

    let mut http_proxy = None;
    let mut https_proxy = None;
    let mut socks_proxy = None;

    for segment in raw.split(';') {
        let segment = segment.trim();
        if let Some(value) = segment.strip_prefix("http=") {
            http_proxy = Some(value.to_string());
        } else if let Some(value) = segment.strip_prefix("https=") {
            https_proxy = Some(value.to_string());
        } else if let Some(value) = segment.strip_prefix("socks=") {
            socks_proxy = Some(value.to_string());
        }
    }

    (http_proxy, https_proxy, socks_proxy)
}

/// Notifies already-running WinINet-based apps (Edge, and anything else
/// using the system WinINet stack) that the proxy settings changed, so they
/// pick up the new values without the user having to log off/reboot.
fn broadcast_settings_changed() -> AppResult<()> {
    unsafe {
        if InternetSetOptionW(std::ptr::null(), INTERNET_OPTION_SETTINGS_CHANGED, std::ptr::null(), 0) == 0 {
            return Err(proxy_error(
                "InternetSetOptionW(INTERNET_OPTION_SETTINGS_CHANGED) failed",
                io::Error::last_os_error(),
            ));
        }
        if InternetSetOptionW(std::ptr::null(), INTERNET_OPTION_REFRESH, std::ptr::null(), 0) == 0 {
            return Err(proxy_error(
                "InternetSetOptionW(INTERNET_OPTION_REFRESH) failed",
                io::Error::last_os_error(),
            ));
        }
    }
    Ok(())
}

fn proxy_error(context: &str, err: impl std::fmt::Display) -> AppError {
    AppError::new("system_proxy_failed", format!("{context}: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_proxy_server_handles_bare_host_port() {
        let (http, https, socks) = parse_proxy_server("127.0.0.1:8080");
        assert_eq!(http.as_deref(), Some("127.0.0.1:8080"));
        assert_eq!(https, None);
        assert_eq!(socks, None);
    }

    #[test]
    fn parse_proxy_server_handles_protocol_specific_form() {
        let (http, https, socks) =
            parse_proxy_server("http=127.0.0.1:18080;https=127.0.0.1:18080;socks=127.0.0.1:18081");
        assert_eq!(http.as_deref(), Some("127.0.0.1:18080"));
        assert_eq!(https.as_deref(), Some("127.0.0.1:18080"));
        assert_eq!(socks.as_deref(), Some("127.0.0.1:18081"));
    }

    #[test]
    fn parse_proxy_server_handles_empty() {
        let (http, https, socks) = parse_proxy_server("");
        assert_eq!(http, None);
        assert_eq!(https, None);
        assert_eq!(socks, None);
    }

    /// Real end-to-end smoke test against this machine's actual registry --
    /// not run by default (`#[ignore]`) since it genuinely flips this
    /// user's system proxy setting. Ends by disabling again so it doesn't
    /// leave the machine pointed at a nonexistent proxy.
    /// Run manually with:
    /// `cargo test -p net --all-targets -- --ignored real_registry`
    #[test]
    #[ignore = "mutates the real HKCU Internet Settings registry key on this machine"]
    fn real_registry_enable_status_disable_lifecycle() {
        enable(18080, 18081).expect("enable should succeed");

        // Confirm the values were really written, independent of status()'s
        // own parsing, so a bug in status() couldn't mask a bug in enable().
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu
            .open_subkey(INTERNET_SETTINGS_PATH)
            .expect("Internet Settings key should exist after enable()");
        let raw_server: String = key.get_value("ProxyServer").expect("ProxyServer should be set");
        assert_eq!(raw_server, "http=127.0.0.1:18080;https=127.0.0.1:18080;socks=127.0.0.1:18081");
        let raw_override: String = key.get_value("ProxyOverride").expect("ProxyOverride should be set");
        assert_eq!(raw_override, DEFAULT_PROXY_OVERRIDE);
        let raw_enable: u32 = key.get_value("ProxyEnable").expect("ProxyEnable should be set");
        assert_eq!(raw_enable, 1);

        let proxy_status = status().expect("status should succeed");
        assert!(proxy_status.enabled);
        assert_eq!(proxy_status.http_proxy.as_deref(), Some("127.0.0.1:18080"));
        assert_eq!(proxy_status.https_proxy.as_deref(), Some("127.0.0.1:18080"));
        assert_eq!(proxy_status.socks_proxy.as_deref(), Some("127.0.0.1:18081"));
        assert!(proxy_status.bypass_list.contains(&"<local>".to_string()));

        disable().expect("disable should succeed");
        let status_after = status().expect("status should succeed");
        assert!(!status_after.enabled, "proxy must be reported disabled: {status_after:?}");
    }
}
