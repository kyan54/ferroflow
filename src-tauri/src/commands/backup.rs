//! Config backup export/import, and a redacted diagnostic-report export.
//!
//! Backup files are the current [`UserConfig`] wrapped in a small versioned
//! envelope (see [`build_backup_json`]) rather than a bare `UserConfig` dump,
//! so a future incompatible format change can be detected on import instead
//! of silently misinterpreted -- see [`parse_backup_json`].
//!
//! Diagnostic reports are a Markdown dump of non-secret app/proxy state,
//! meant to be pasted into a GitHub issue -- see [`render_diagnostic_report`]
//! for exactly what's redacted vs. not.
//!
//! Unlike most other command modules, the path a file gets written to/read
//! from is chosen by the frontend via the native save/open dialog
//! (`@tauri-apps/plugin-dialog`) -- these commands just do the read/write at
//! whatever `path` they're given, no dialog logic on the Rust side.

use std::fmt::Write as _;

use serde_json::json;
use shared_types::{
    AppError, AppResult, HelperStatus, PlatformInfo, ProxyStatus, SystemProxyStatus, UserConfig,
};
use tauri::{AppHandle, Manager, State};

use crate::state::{save_persisted_config, AppState};

/// Current backup envelope version. Bump this and add a migration branch in
/// [`parse_backup_json`] if `UserConfig`'s shape ever changes incompatibly
/// enough that an older backup can no longer be loaded as-is.
const BACKUP_VERSION: u64 = 1;

/// Builds the on-disk JSON envelope for a config backup:
/// `{"ferroflowBackupVersion": 1, "config": <UserConfig, camelCase>}`.
///
/// Pulled out of [`backup_export`] so it's unit-testable without a
/// `tauri::AppHandle`.
fn build_backup_json(config: &UserConfig) -> serde_json::Value {
    json!({
        "ferroflowBackupVersion": BACKUP_VERSION,
        "config": config,
    })
}

/// Parses a backup file's raw contents back into a `UserConfig`, checking
/// the envelope version first. Anything other than exactly
/// [`BACKUP_VERSION`] is rejected with `backup_incompatible` rather than
/// guess-migrated -- that's a future problem, once there's a second version
/// to migrate from.
///
/// Pulled out of [`backup_import`] so it's unit-testable without a
/// `tauri::AppHandle`/`State`.
fn parse_backup_json(raw: &str) -> AppResult<UserConfig> {
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| AppError::new("backup_invalid", format!("not valid JSON: {e}")))?;

    let version = value
        .get("ferroflowBackupVersion")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| {
            AppError::new("backup_invalid", "missing or non-numeric ferroflowBackupVersion field")
        })?;

    if version != BACKUP_VERSION {
        return Err(AppError::new(
            "backup_incompatible",
            format!(
                "backup file is version {version}, this build only supports version {BACKUP_VERSION}"
            ),
        ));
    }

    let config_value = value
        .get("config")
        .ok_or_else(|| AppError::new("backup_invalid", "missing config field"))?;

    serde_json::from_value(config_value.clone()).map_err(|e| {
        AppError::new("backup_invalid", format!("config field doesn't match UserConfig: {e}"))
    })
}

/// Serializes a serde-serializable value to its plain wire representation
/// (e.g. a unit enum with `rename_all = "..."` becomes its bare tag, not a
/// quoted JSON string) for readable Markdown output.
fn ser_str<T: serde::Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => s,
        Ok(other) => other.to_string(),
        Err(_) => "?".to_string(),
    }
}

/// Renders the redacted Markdown diagnostic report. Meant to be pasted into
/// a GitHub issue, so everything in it must be safe for that:
///
/// - **Servers**: `name`/`protocol`/`address`/`port`/`tls.enabled`/
///   `tls.server_name` are shown (useful for diagnosing a connection issue);
///   `uuid`/`password`/`reality_public_key`/`reality_short_id` are never
///   included at all.
/// - **Rules**: shown in full -- domains/IPs/process names are the entire
///   point of a rule, not a secret.
/// - **Settings**: the non-secret `UserConfig` fields (proxy mode/type,
///   auto-start, etc.) are shown in full; there's nothing secret in them.
fn render_diagnostic_report(
    version: &str,
    platform: &PlatformInfo,
    config: &UserConfig,
    proxy_status: &ProxyStatus,
    system_proxy_status: &SystemProxyStatus,
    helper_status: Option<&HelperStatus>,
) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "# Ferroflow diagnostic report");
    let _ = writeln!(out);

    let _ = writeln!(out, "## App");
    let _ = writeln!(out, "- Version: {version}");
    let _ = writeln!(out);

    let _ = writeln!(out, "## Platform");
    let _ = writeln!(out, "- Platform: {}", ser_str(&platform.platform));
    let _ = writeln!(out, "- Arch: {}", platform.arch);
    let _ = writeln!(
        out,
        "- OS version: {}",
        if platform.os_version.is_empty() { "(unknown)" } else { platform.os_version.as_str() }
    );
    let _ = writeln!(out, "- Running as admin: {}", platform.is_admin);
    let _ = writeln!(out);

    let _ = writeln!(out, "## Settings");
    let _ = writeln!(out, "- Proxy mode: {}", ser_str(&config.proxy_mode));
    let _ = writeln!(out, "- Proxy mode type: {}", ser_str(&config.proxy_mode_type));
    let _ = writeln!(out, "- Auto start: {}", config.auto_start);
    let _ = writeln!(out, "- Silent start: {}", config.silent_start);
    let _ = writeln!(out, "- Auto connect: {}", config.auto_connect);
    let _ = writeln!(out, "- Minimize to tray: {}", config.minimize_to_tray);
    let _ = writeln!(out, "- Language: {}", config.language.as_deref().unwrap_or("(default)"));
    let _ = writeln!(
        out,
        "- Selected server id: {}",
        config.selected_server_id.as_deref().unwrap_or("(none)")
    );
    let _ = writeln!(out);

    let _ = writeln!(out, "## Servers ({} total, secrets redacted)", config.servers.len());
    if config.servers.is_empty() {
        let _ = writeln!(out, "_none configured_");
    } else {
        let _ = writeln!(out, "| Name | Protocol | Address | Port | TLS enabled | TLS SNI |");
        let _ = writeln!(out, "|---|---|---|---|---|---|");
        for s in &config.servers {
            let tls_enabled = s.tls.as_ref().map(|t| t.enabled).unwrap_or(false);
            let tls_sni = s.tls.as_ref().and_then(|t| t.server_name.as_deref()).unwrap_or("-");
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} | {} |",
                s.name,
                ser_str(&s.protocol),
                s.address,
                s.port,
                tls_enabled,
                tls_sni
            );
        }
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "Redacted (present in the real config, stripped from this report): `uuid`, \
             `password`, `tls.reality_public_key`, `tls.reality_short_id`."
        );
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## Rules ({} total)", config.rules.len());
    if config.rules.is_empty() {
        let _ = writeln!(out, "_none configured_");
    } else {
        let _ = writeln!(out, "| Name | Enabled | Match type | Values | Outbound |");
        let _ = writeln!(out, "|---|---|---|---|---|");
        for r in &config.rules {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} |",
                r.name,
                r.enabled,
                ser_str(&r.match_type),
                r.values.join(", "),
                ser_str(&r.outbound)
            );
        }
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## Proxy status");
    let _ = writeln!(out, "- Running: {}", proxy_status.running);
    let _ = writeln!(
        out,
        "- PID: {}",
        proxy_status.pid.map(|p| p.to_string()).unwrap_or_else(|| "-".to_string())
    );
    let _ = writeln!(
        out,
        "- Uptime (s): {}",
        proxy_status.uptime_secs.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string())
    );
    let _ = writeln!(
        out,
        "- Current server id: {}",
        proxy_status.current_server_id.as_deref().unwrap_or("-")
    );
    let _ = writeln!(
        out,
        "- Local port: {}",
        proxy_status.local_port.map(|p| p.to_string()).unwrap_or_else(|| "-".to_string())
    );
    if let Some(err) = &proxy_status.error {
        let _ = writeln!(out, "- Error: {err}");
    }
    if let Some(code) = &proxy_status.error_code {
        let _ = writeln!(out, "- Error code: {}", ser_str(code));
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## System proxy status");
    let _ = writeln!(out, "- Enabled: {}", system_proxy_status.enabled);
    let _ = writeln!(
        out,
        "- HTTP proxy: {}",
        system_proxy_status.http_proxy.as_deref().unwrap_or("-")
    );
    let _ = writeln!(
        out,
        "- HTTPS proxy: {}",
        system_proxy_status.https_proxy.as_deref().unwrap_or("-")
    );
    let _ = writeln!(
        out,
        "- SOCKS proxy: {}",
        system_proxy_status.socks_proxy.as_deref().unwrap_or("-")
    );
    let _ = writeln!(
        out,
        "- Bypass list: {}",
        if system_proxy_status.bypass_list.is_empty() {
            "-".to_string()
        } else {
            system_proxy_status.bypass_list.join(", ")
        }
    );
    let _ = writeln!(out);

    let _ = writeln!(out, "## Helper status");
    match helper_status {
        Some(h) => {
            let _ = writeln!(out, "- Platform: {}", ser_str(&h.platform));
            let _ = writeln!(out, "- Installed: {}", h.installed);
            let _ = writeln!(out, "- Ready: {}", h.ready);
            let _ = writeln!(out, "- Version: {}", h.version.as_deref().unwrap_or("-"));
            let _ = writeln!(out, "- Needs repair: {}", h.needs_repair);
        }
        None => {
            let _ = writeln!(out, "_unavailable (helper status check failed or timed out)_");
        }
    }

    out
}

/// Writes the current config to `path` as a versioned JSON backup. `path` is
/// chosen by the frontend's native save dialog -- see the module doc comment.
///
/// Takes `AppHandle` rather than `State<AppState>` (unlike every other
/// mutating command in this codebase) to match this command's required
/// signature; `AppState` is reached the same way `state::set_persisted_helper_token`
/// already does, via `app.state::<AppState>()`.
#[tauri::command]
pub async fn backup_export(app: AppHandle, path: String) -> AppResult<()> {
    let config = app.state::<AppState>().config.lock().unwrap().clone();
    let envelope = build_backup_json(&config);
    let bytes = serde_json::to_vec_pretty(&envelope)
        .map_err(|e| AppError::new("backup_export_failed", format!("failed to serialize config: {e}")))?;
    std::fs::write(&path, bytes)
        .map_err(|e| AppError::new("backup_export_failed", format!("failed to write {path}: {e}")))?;
    Ok(())
}

/// Reads a versioned JSON backup from `path`, replaces the current config
/// with it, and persists that as the new `config.json` -- mirrors
/// `commands::config::config_save`'s exact lock/persist pattern.
#[tauri::command]
pub async fn backup_import(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<UserConfig> {
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| AppError::new("backup_invalid", format!("failed to read {path}: {e}")))?;
    let config = parse_backup_json(&raw)?;

    *state.config.lock().unwrap() = config.clone();
    save_persisted_config(&app, &config)
        .map_err(|e| AppError::new("config_write_failed", e.to_string()))?;
    Ok(config)
}

/// Writes a redacted Markdown diagnostic report to `path`, meant to be
/// pasted into a GitHub issue -- see [`render_diagnostic_report`] for the
/// exact redaction rules.
#[tauri::command]
pub async fn diagnostic_export(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<()> {
    let config = state.config.lock().unwrap().clone();
    let platform = crate::commands::system::platform_info()?;
    let proxy_status = state.core_manager.status().await?;
    let system_proxy_status = state.system_proxy.status()?;
    // Best-effort: reuses the same platform-dispatching command the frontend
    // calls (`commands::helper::helper_get_status`) rather than duplicating
    // its per-OS logic here. A failure here (e.g. helper genuinely
    // unreachable) just omits the section below rather than failing the
    // whole report -- a diagnostic export shouldn't itself become
    // undiagnosable because a secondary status check errored.
    let helper_status = crate::commands::helper::helper_get_status(state.clone()).await.ok();

    let report = render_diagnostic_report(
        env!("CARGO_PKG_VERSION"),
        &platform,
        &config,
        &proxy_status,
        &system_proxy_status,
        helper_status.as_ref(),
    );

    let _ = &app;
    std::fs::write(&path, report).map_err(|e| {
        AppError::new("diagnostic_export_failed", format!("failed to write {path}: {e}"))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_types::{Protocol, ProxyMode, ProxyModeType, RoutingRule, RuleMatchType, RuleOutbound, ServerConfig, TlsConfig};

    fn sample_config() -> UserConfig {
        UserConfig {
            servers: vec![ServerConfig {
                id: "srv-1".to_string(),
                name: "My Server".to_string(),
                protocol: Protocol::Vless,
                address: "example.com".to_string(),
                port: 443,
                uuid: Some("secret-uuid-1234".to_string()),
                password: Some("secret-password-5678".to_string()),
                encryption: None,
                flow: None,
                tls: Some(TlsConfig {
                    enabled: true,
                    server_name: Some("sni.example.com".to_string()),
                    insecure: false,
                    reality_public_key: Some("secret-reality-pubkey".to_string()),
                    reality_short_id: Some("secret-reality-shortid".to_string()),
                }),
            }],
            rules: vec![RoutingRule {
                id: "rule-1".to_string(),
                name: "Direct CN".to_string(),
                enabled: true,
                match_type: RuleMatchType::DomainSuffix,
                values: vec![".cn".to_string()],
                outbound: RuleOutbound::Direct,
            }],
            selected_server_id: Some("srv-1".to_string()),
            proxy_mode: ProxyMode::Smart,
            proxy_mode_type: ProxyModeType::SystemProxy,
            auto_start: true,
            silent_start: false,
            auto_connect: true,
            minimize_to_tray: true,
            language: Some("en".to_string()),
        }
    }

    #[test]
    fn backup_round_trip_preserves_config() {
        let config = sample_config();
        let envelope = build_backup_json(&config);
        let raw = serde_json::to_string(&envelope).expect("serialize envelope");

        let parsed = parse_backup_json(&raw).expect("parse backup");

        // UserConfig doesn't derive PartialEq; compare via their JSON
        // representations instead, which is exactly the round-trip that
        // matters here (backup -> disk -> import).
        assert_eq!(
            serde_json::to_value(&parsed).unwrap(),
            serde_json::to_value(&config).unwrap()
        );
    }

    #[test]
    fn backup_import_rejects_unknown_version() {
        let raw = r#"{"ferroflowBackupVersion": 2, "config": {}}"#;
        let err = parse_backup_json(raw).expect_err("should reject unknown version");
        assert_eq!(err.code, "backup_incompatible");
    }

    #[test]
    fn backup_import_rejects_malformed_json() {
        let raw = "{ this is not valid json";
        let err = parse_backup_json(raw).expect_err("should reject malformed json");
        assert_eq!(err.code, "backup_invalid");
    }

    #[test]
    fn backup_import_rejects_missing_envelope_fields() {
        let raw = r#"{"config": {}}"#;
        let err = parse_backup_json(raw).expect_err("should reject missing version field");
        assert_eq!(err.code, "backup_invalid");

        let raw_no_config = r#"{"ferroflowBackupVersion": 1}"#;
        let err = parse_backup_json(raw_no_config).expect_err("should reject missing config field");
        assert_eq!(err.code, "backup_invalid");
    }

    #[test]
    fn diagnostic_report_redacts_secrets_but_keeps_useful_fields() {
        let config = sample_config();
        let platform = PlatformInfo {
            platform: shared_types::HelperPlatform::Windows,
            arch: "x86_64".to_string(),
            os_version: "Windows 11".to_string(),
            is_admin: false,
        };
        let proxy_status = ProxyStatus::default();
        let system_proxy_status = SystemProxyStatus::default();

        let report = render_diagnostic_report(
            "0.1.0",
            &platform,
            &config,
            &proxy_status,
            &system_proxy_status,
            None,
        );

        // Secrets must never appear.
        assert!(!report.contains("secret-uuid-1234"));
        assert!(!report.contains("secret-password-5678"));
        assert!(!report.contains("secret-reality-pubkey"));
        assert!(!report.contains("secret-reality-shortid"));

        // Useful, non-secret fields must still be present.
        assert!(report.contains("My Server"));
        assert!(report.contains("example.com"));
        assert!(report.contains("sni.example.com"));
        assert!(report.contains("443"));
        assert!(report.contains("vless"));

        // Rules have no secrets, so they're shown in full.
        assert!(report.contains("Direct CN"));
        assert!(report.contains(".cn"));
    }
}
