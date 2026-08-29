//! macOS system-proxy via `networksetup -setwebproxy/-setsocksfirewallproxy`
//! per active network service.
//!
//! A machine can have more than one *active* network service at once (e.g.
//! Wi-Fi and Ethernet both up, or a VPN service layered on top of the
//! physical interface) — `networksetup` proxy settings are per-service, so
//! setting only one leaves traffic on the others un-proxied. Every function
//! here therefore enumerates `active_services()` and applies/reads the
//! setting against all of them (`enable`/`disable`) or the first one
//! (`status`, which reports a single system-wide-ish view the same way
//! macOS's own Network preference pane does per-service).

use std::process::Command;

use shared_types::{AppError, AppResult, SystemProxyStatus};

/// Runs `networksetup -listallnetworkservices` and returns the names of
/// services that are currently enabled.
///
/// The command's output is one service name per line, except:
/// - the first line, a fixed informational string starting with `"An
///   asterisk (*)"`, which is never a service name;
/// - any line starting with `*`, which denotes a *disabled* service (the
///   asterisk is `networksetup`'s own convention, not something we add).
fn active_services() -> Vec<String> {
    let output = match Command::new("networksetup").arg("-listallnetworkservices").output() {
        Ok(output) => output,
        Err(e) => {
            tracing::warn!("failed to run networksetup -listallnetworkservices: {e}");
            return Vec::new();
        }
    };

    if !output.status.success() {
        tracing::warn!(
            "networksetup -listallnetworkservices exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        return Vec::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .skip(1) // the "An asterisk (*) denotes..." info line
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('*'))
        .map(str::to_string)
        .collect()
}

/// Runs a single `networksetup` subcommand against `service`, logging (and
/// swallowing) any failure so the caller can keep going with the next
/// service/command. Returns whether it succeeded, for callers that need to
/// track best-effort success across a batch.
fn run_networksetup(args: &[&str]) -> bool {
    match Command::new("networksetup").args(args).output() {
        Ok(output) if output.status.success() => true,
        Ok(output) => {
            tracing::warn!(
                "networksetup {} exited with {}: {}",
                args.join(" "),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
            false
        }
        Err(e) => {
            tracing::warn!("failed to run networksetup {}: {e}", args.join(" "));
            false
        }
    }
}

pub(crate) fn enable(http_port: u16, socks_port: u16) -> AppResult<()> {
    let services = active_services();
    if services.is_empty() {
        return Err(AppError::new("system_proxy_failed", "no active network services found"));
    }

    let http_port = http_port.to_string();
    let socks_port = socks_port.to_string();
    let mut any_succeeded = false;

    for service in &services {
        // Best-effort per service: a single misconfigured/odd service
        // shouldn't block proxying on the ones that DO work. Track whether
        // every command for this service succeeded so we know whether at
        // least one whole service came up proxied.
        let mut service_ok = true;
        service_ok &= run_networksetup(&["-setwebproxy", service, "127.0.0.1", &http_port]);
        service_ok &= run_networksetup(&["-setsecurewebproxy", service, "127.0.0.1", &http_port]);
        service_ok &= run_networksetup(&["-setsocksfirewallproxy", service, "127.0.0.1", &socks_port]);
        service_ok &= run_networksetup(&["-setwebproxystate", service, "on"]);
        service_ok &= run_networksetup(&["-setsecurewebproxystate", service, "on"]);
        service_ok &= run_networksetup(&["-setsocksfirewallproxystate", service, "on"]);
        service_ok &= run_networksetup(&[
            "-setproxybypassdomains",
            service,
            "127.0.0.1",
            "localhost",
            "<local>",
        ]);

        if service_ok {
            any_succeeded = true;
        }
    }

    if any_succeeded {
        Ok(())
    } else {
        Err(AppError::new(
            "system_proxy_failed",
            "failed to enable the system proxy on every active network service",
        ))
    }
}

pub(crate) fn disable() -> AppResult<()> {
    let services = active_services();

    // Nothing to turn off is not an error -- `disable()` must be idempotent
    // (e.g. called when the proxy was never enabled, or a fresh machine with
    // no active services at all).
    for service in &services {
        run_networksetup(&["-setwebproxystate", service, "off"]);
        run_networksetup(&["-setsecurewebproxystate", service, "off"]);
        run_networksetup(&["-setsocksfirewallproxystate", service, "off"]);
    }

    Ok(())
}

/// Parses `networksetup -get{web,secureweb,socksfirewall}proxy`'s output
/// (`Key: Value` lines) into `(enabled, server, port)`. Split out from
/// `status()` so it can be unit-tested without actually running
/// `networksetup`.
fn parse_getproxy_output(output: &str) -> (bool, Option<String>, Option<u16>) {
    let mut enabled = false;
    let mut server = None;
    let mut port = None;

    for line in output.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let value = value.trim();

        match key.trim() {
            "Enabled" => enabled = value.eq_ignore_ascii_case("yes"),
            "Server" if !value.is_empty() => {
                server = Some(value.to_string());
            }
            "Port" if !value.is_empty() => {
                port = value.parse::<u16>().ok();
            }
            _ => {}
        }
    }

    (enabled, server, port)
}

/// Runs `networksetup <get_flag> <service>` and returns its raw stdout, or
/// `None` if the command couldn't be run/failed.
fn run_getproxy(get_flag: &str, service: &str) -> Option<String> {
    match Command::new("networksetup").args([get_flag, service]).output() {
        Ok(output) if output.status.success() => {
            Some(String::from_utf8_lossy(&output.stdout).to_string())
        }
        Ok(output) => {
            tracing::warn!(
                "networksetup {get_flag} {service} exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
            None
        }
        Err(e) => {
            tracing::warn!("failed to run networksetup {get_flag} {service}: {e}");
            None
        }
    }
}

fn format_proxy(server: Option<String>, port: Option<u16>) -> Option<String> {
    match (server, port) {
        (Some(server), Some(port)) => Some(format!("{server}:{port}")),
        (Some(server), None) => Some(server),
        _ => None,
    }
}

pub(crate) fn status() -> AppResult<SystemProxyStatus> {
    let services = active_services();
    let Some(service) = services.first() else {
        return Ok(SystemProxyStatus::default());
    };

    let (enabled, http_server, http_port) = run_getproxy("-getwebproxy", service)
        .map(|out| parse_getproxy_output(&out))
        .unwrap_or_default();
    let (_, https_server, https_port) = run_getproxy("-getsecurewebproxy", service)
        .map(|out| parse_getproxy_output(&out))
        .unwrap_or_default();
    let (_, socks_server, socks_port) = run_getproxy("-getsocksfirewallproxy", service)
        .map(|out| parse_getproxy_output(&out))
        .unwrap_or_default();

    let bypass_list = run_getproxy("-getproxybypassdomains", service)
        .map(|out| out.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect())
        .unwrap_or_default();

    Ok(SystemProxyStatus {
        enabled,
        http_proxy: format_proxy(http_server, http_port),
        https_proxy: format_proxy(https_server, https_port),
        socks_proxy: format_proxy(socks_server, socks_port),
        bypass_list,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_getproxy_output_enabled_with_server_and_port() {
        let output = "Enabled: Yes\nServer: 127.0.0.1\nPort: 8080\nAuthenticated Proxy Enabled: 0\n";
        let (enabled, server, port) = parse_getproxy_output(output);
        assert!(enabled);
        assert_eq!(server.as_deref(), Some("127.0.0.1"));
        assert_eq!(port, Some(8080));
    }

    #[test]
    fn parse_getproxy_output_disabled() {
        let output = "Enabled: No\nServer: \nPort: 0\nAuthenticated Proxy Enabled: 0\n";
        let (enabled, _server, _port) = parse_getproxy_output(output);
        assert!(!enabled);
    }

    #[test]
    fn parse_getproxy_output_empty_server_and_port_does_not_panic() {
        let output = "Enabled: No\nServer: \nPort: \nAuthenticated Proxy Enabled: 0\n";
        let (enabled, server, port) = parse_getproxy_output(output);
        assert!(!enabled);
        assert_eq!(server, None);
        assert_eq!(port, None);
    }

    #[test]
    fn parse_getproxy_output_ignores_unknown_lines_and_blank_lines() {
        let output = "\nEnabled: Yes\nSomethingElse: whatever\nServer: proxy.example.com\nPort: 3128\n";
        let (enabled, server, port) = parse_getproxy_output(output);
        assert!(enabled);
        assert_eq!(server.as_deref(), Some("proxy.example.com"));
        assert_eq!(port, Some(3128));
    }

    #[test]
    fn format_proxy_combines_server_and_port() {
        assert_eq!(format_proxy(Some("127.0.0.1".into()), Some(8080)), Some("127.0.0.1:8080".into()));
        assert_eq!(format_proxy(Some("127.0.0.1".into()), None), Some("127.0.0.1".into()));
        assert_eq!(format_proxy(None, Some(8080)), None);
        assert_eq!(format_proxy(None, None), None);
    }
}
