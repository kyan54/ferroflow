//! Linux system-proxy via `gsettings` (the GNOME/GTK desktop proxy schema,
//! `org.gnome.system.proxy*`) -- no crate needed, this just shells out to
//! the `gsettings` binary.
//!
//! ## Scope limitation (read before assuming this "just works")
//!
//! This only ever changes GNOME's own proxy settings. That means:
//! - It affects most GTK apps, and Chrome/Chromium when launched under a
//!   GNOME session (both read `org.gnome.system.proxy` directly).
//! - It does **not** affect terminal/CLI tools that rely on
//!   `http_proxy`/`https_proxy`/`all_proxy` environment variables. Those are
//!   read once at process startup from that process's own environment --
//!   there is no OS-level "environment variable registry" a `gsettings set`
//!   (or anything else) can push into. An already-running process's
//!   environment cannot be changed from outside it, full stop. Exporting the
//!   var in *this* process wouldn't help either: env vars only flow
//!   one-directional, parent-to-child, at spawn time, so it would propagate
//!   to children this process spawns afterward and nothing else -- not to
//!   already-running processes, not to sibling processes, not retroactively
//!   to anything. There is deliberately no "env var fallback" attempted
//!   here, because there is no version of one that would produce an
//!   observable effect for the user.
//! - It does **not** affect non-GNOME desktop environments (KDE, Xfce,
//!   sway/wlroots compositors, i3, etc.) at all -- each of those has its own
//!   proxy mechanism (`kwriteconfig`/KDE's `kioslaverc`, no standard at all
//!   for many WMs) that is simply out of scope for this MVP pass. No
//!   fallback is attempted for those either; `enable`/`disable` report a
//!   clear error when `gsettings` itself isn't usable, and `status` reports
//!   "not enabled" rather than guessing.
//!
//! Given that, "system proxy" on Linux in this app should be understood as
//! "GNOME's system proxy," not "every process's network I/O."

use std::process::Command;

use shared_types::{AppError, AppResult, SystemProxyStatus};

const GSETTINGS_BIN: &str = "gsettings";

const PROXY_SCHEMA: &str = "org.gnome.system.proxy";
const HTTP_SCHEMA: &str = "org.gnome.system.proxy.http";
const HTTPS_SCHEMA: &str = "org.gnome.system.proxy.https";
const SOCKS_SCHEMA: &str = "org.gnome.system.proxy.socks";

/// Runs `gsettings <args>`, treating both "binary not found/failed to spawn"
/// and "ran but exited non-zero" (e.g. the `org.gnome.system.proxy` schema
/// doesn't exist because this isn't a GNOME session) as failure -- callers
/// don't need to distinguish the two, both mean "this mechanism isn't usable
/// here." Returns trimmed stdout on success.
fn run_gsettings(args: &[&str]) -> Result<String, String> {
    match Command::new(GSETTINGS_BIN).args(args).output() {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!(
                "gsettings {} exited with {}: {}",
                args.join(" "),
                output.status,
                stderr.trim()
            ))
        }
        Err(e) => Err(format!("failed to run '{GSETTINGS_BIN}': {e}")),
    }
}

/// Like `run_gsettings`, but for reads that should degrade to `None` rather
/// than fail the whole `status()` call -- a single stale/missing key
/// shouldn't hide the rest of an otherwise-readable status.
fn run_gsettings_lenient(args: &[&str]) -> Option<String> {
    match run_gsettings(args) {
        Ok(value) => Some(value),
        Err(e) => {
            tracing::warn!("gsettings {} failed: {e}", args.join(" "));
            None
        }
    }
}

/// Strips a GVariant single-quoted string's surrounding quotes, e.g.
/// `'manual'` -> `manual`. This is GVariant's text format for strings (what
/// `gsettings get` prints for a string-typed key), not shell quoting -- if
/// the input doesn't look like a quoted string, it's returned trimmed and
/// otherwise unchanged rather than panicking, so a malformed/empty upstream
/// value degrades gracefully instead of corrupting the result.
fn parse_quoted_string(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Parses a GVariant array-of-strings text form, e.g.
/// `['localhost', '127.0.0.0/8', '::1']`, into
/// `vec!["localhost", "127.0.0.0/8", "::1"]` (and `[]` into an empty vec).
///
/// This is a small hand-rolled parser rather than a naive
/// `split(',')` because GVariant's text format allows a quoted element to
/// contain a literal comma (`'foo,bar'`) or an escaped apostrophe (`'it\'s'`)
/// -- a plain split would misparse both. It tracks whether it's inside a
/// quoted element and only treats `,` as a separator outside quotes, and
/// unescapes a backslash-prefixed character (GVariant's own escaping for
/// `'`/`\` inside a quoted string) by passing the escaped character through
/// literally.
fn parse_gvariant_string_array(s: &str) -> Vec<String> {
    let s = s.trim();
    let inner = s.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')).unwrap_or(s).trim();
    if inner.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' if in_quotes => {
                if let Some(escaped) = chars.next() {
                    current.push(escaped);
                }
            }
            '\'' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                result.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    result.push(current.trim().to_string());
    result
}

/// Combines a `gsettings get <schema>.http host` / `... port` pair (or
/// `.https`/`.socks`) into a single `"host:port"` string for
/// `SystemProxyStatus`'s flattened fields. `None` if either half is
/// unavailable -- a partial reading isn't reportable as a proxy address.
fn combine_host_port(host: Option<String>, port: Option<String>) -> Option<String> {
    let host = parse_quoted_string(&host?);
    let port = port?.trim().to_string();
    Some(format!("{host}:{port}"))
}

fn gsettings_not_available_err(context: &str) -> AppError {
    AppError::new(
        "system_proxy_failed",
        format!("gsettings is not available (not a GNOME/GTK desktop session?): {context}"),
    )
}

/// Points GNOME's proxy settings at `127.0.0.1:<http_port>` for HTTP/HTTPS
/// and `127.0.0.1:<socks_port>` for SOCKS, and switches proxy mode to
/// `'manual'` so GNOME (and GTK/GNOME-aware apps) actually start using them.
/// See the module doc comment for exactly which apps that reaches.
///
/// The first command (switching `mode` to `'manual'`) is treated as a
/// canary: if it fails to even run -- `gsettings` isn't installed, or the
/// `org.gnome.system.proxy` schema doesn't exist -- this isn't a GNOME
/// session at all, so this returns `Err` immediately rather than running six
/// more commands that would also fail. Once that canary succeeds, the
/// remaining host/port/ignore-hosts writes are best-effort: a failure on any
/// one of them is logged and does not stop the others from being attempted,
/// since e.g. a failure setting `ignore-hosts` shouldn't prevent the
/// actually-important host/port settings from taking effect.
pub(crate) fn enable(http_port: u16, socks_port: u16) -> AppResult<()> {
    run_gsettings(&["set", PROXY_SCHEMA, "mode", "manual"])
        .map_err(|e| gsettings_not_available_err(&e))?;

    let http_port = http_port.to_string();
    let socks_port = socks_port.to_string();

    let best_effort: [&[&str]; 6] = [
        &["set", HTTP_SCHEMA, "host", "127.0.0.1"],
        &["set", HTTP_SCHEMA, "port", &http_port],
        &["set", HTTPS_SCHEMA, "host", "127.0.0.1"],
        &["set", HTTPS_SCHEMA, "port", &http_port],
        &["set", SOCKS_SCHEMA, "host", "127.0.0.1"],
        &["set", SOCKS_SCHEMA, "port", &socks_port],
    ];
    for args in best_effort {
        if let Err(e) = run_gsettings(args) {
            tracing::warn!("gsettings {} failed (continuing): {e}", args.join(" "));
        }
    }

    // GVariant array-of-strings syntax: a bracketed, comma-separated list of
    // single-quoted strings. Passed as one argument (not through a shell),
    // so there's no extra shell-quoting layer to get wrong here.
    if let Err(e) = run_gsettings(&[
        "set",
        PROXY_SCHEMA,
        "ignore-hosts",
        "['localhost', '127.0.0.0/8', '::1']",
    ]) {
        tracing::warn!("gsettings set {PROXY_SCHEMA} ignore-hosts failed (continuing): {e}");
    }

    Ok(())
}

/// Switches GNOME's proxy mode back to `'none'`. Idempotent -- succeeds even
/// if it's already `'none'` (`gsettings set` doesn't error on a no-op
/// write), matching `SystemProxyManager::disable`'s documented contract.
/// Same "gsettings not available" -> `Err` handling as `enable`, but this
/// only ever needs to touch the one `mode` key.
pub(crate) fn disable() -> AppResult<()> {
    run_gsettings(&["set", PROXY_SCHEMA, "mode", "none"])
        .map_err(|e| gsettings_not_available_err(&e))?;
    Ok(())
}

/// Reads GNOME's actual current proxy configuration. If `gsettings` isn't
/// runnable at all (not found -- no GNOME session), this reports
/// `SystemProxyStatus::default()` rather than an error: "no GNOME session"
/// just means proxy-via-this-mechanism is obviously not enabled, which is
/// exactly what the default represents, matching this codebase's pattern of
/// `status()` reporting a safe default instead of erroring when there's
/// nothing to report.
pub(crate) fn status() -> AppResult<SystemProxyStatus> {
    let mode_raw = match run_gsettings(&["get", PROXY_SCHEMA, "mode"]) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("gsettings not available, reporting default system proxy status: {e}");
            return Ok(SystemProxyStatus::default());
        }
    };

    if parse_quoted_string(&mode_raw) != "manual" {
        return Ok(SystemProxyStatus::default());
    }

    let http_host = run_gsettings_lenient(&["get", HTTP_SCHEMA, "host"]);
    let http_port = run_gsettings_lenient(&["get", HTTP_SCHEMA, "port"]);
    let https_host = run_gsettings_lenient(&["get", HTTPS_SCHEMA, "host"]);
    let https_port = run_gsettings_lenient(&["get", HTTPS_SCHEMA, "port"]);
    let socks_host = run_gsettings_lenient(&["get", SOCKS_SCHEMA, "host"]);
    let socks_port = run_gsettings_lenient(&["get", SOCKS_SCHEMA, "port"]);
    let ignore_hosts_raw = run_gsettings_lenient(&["get", PROXY_SCHEMA, "ignore-hosts"]);

    Ok(SystemProxyStatus {
        enabled: true,
        http_proxy: combine_host_port(http_host, http_port),
        https_proxy: combine_host_port(https_host, https_port),
        socks_proxy: combine_host_port(socks_host, socks_port),
        bypass_list: ignore_hosts_raw.map(|s| parse_gvariant_string_array(&s)).unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- parse_quoted_string ------------------------------------------------

    #[test]
    fn parse_quoted_string_manual() {
        assert_eq!(parse_quoted_string("'manual'"), "manual");
    }

    #[test]
    fn parse_quoted_string_none() {
        assert_eq!(parse_quoted_string("'none'"), "none");
    }

    #[test]
    fn parse_quoted_string_trims_surrounding_whitespace() {
        assert_eq!(parse_quoted_string("  'manual'\n"), "manual");
    }

    #[test]
    fn parse_quoted_string_without_quotes_returned_as_is() {
        // Defensive fallback -- shouldn't happen for a well-formed
        // `gsettings get` on a string key, but shouldn't panic either.
        assert_eq!(parse_quoted_string("manual"), "manual");
    }

    #[test]
    fn parse_quoted_string_empty_input() {
        assert_eq!(parse_quoted_string(""), "");
    }

    // -- parse_gvariant_string_array -----------------------------------------

    #[test]
    fn parse_gvariant_array_empty() {
        let result: Vec<String> = parse_gvariant_string_array("[]");
        assert_eq!(result, Vec::<String>::new());
    }

    #[test]
    fn parse_gvariant_array_single_element() {
        assert_eq!(parse_gvariant_string_array("['localhost']"), vec!["localhost"]);
    }

    #[test]
    fn parse_gvariant_array_multiple_elements() {
        assert_eq!(
            parse_gvariant_string_array("['localhost', '127.0.0.0/8', '::1']"),
            vec!["localhost", "127.0.0.0/8", "::1"]
        );
    }

    #[test]
    fn parse_gvariant_array_no_spaces_after_commas() {
        assert_eq!(
            parse_gvariant_string_array("['a','b','c']"),
            vec!["a", "b", "c"]
        );
    }

    #[test]
    fn parse_gvariant_array_element_containing_comma() {
        assert_eq!(
            parse_gvariant_string_array("['foo,bar', 'baz']"),
            vec!["foo,bar", "baz"]
        );
    }

    #[test]
    fn parse_gvariant_array_element_containing_escaped_apostrophe() {
        // GVariant's text format escapes an apostrophe inside a
        // single-quoted string as `\'`.
        assert_eq!(parse_gvariant_string_array(r"['it\'s', 'ok']"), vec!["it's", "ok"]);
    }

    #[test]
    fn parse_gvariant_array_trims_whitespace_around_input() {
        assert_eq!(
            parse_gvariant_string_array("  ['localhost']\n"),
            vec!["localhost"]
        );
    }

    // -- combine_host_port ----------------------------------------------------

    #[test]
    fn combine_host_port_both_present() {
        assert_eq!(
            combine_host_port(Some("'127.0.0.1'".into()), Some("8080".into())),
            Some("127.0.0.1:8080".into())
        );
    }

    #[test]
    fn combine_host_port_missing_host_is_none() {
        assert_eq!(combine_host_port(None, Some("8080".into())), None);
    }

    #[test]
    fn combine_host_port_missing_port_is_none() {
        assert_eq!(combine_host_port(Some("'127.0.0.1'".into()), None), None);
    }
}
