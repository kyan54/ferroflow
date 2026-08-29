//! Subscription-link import: fetches a provider's subscription URL, parses
//! whatever share-links it contains (see `subscription::parse` for the
//! supported `vless://`/`trojan://`/`ss://`/`vmess://` formats), appends the
//! newly parsed servers to the persisted config, and returns the updated
//! `UserConfig` -- same save-then-return-clone shape as
//! `commands::config::servers_add`.
//!
//! Three import entry points share the same "parse to `Vec<ServerConfig>`,
//! then append+persist+return" tail (`import_servers` below):
//! - `subscription_import`: fetches a URL first (network).
//! - `subscription_import_text`: pasted raw text (one or more share-links,
//!   optionally base64-wrapped) -- no network, no filesystem.
//! - `subscription_import_file`: a local file the user picked via the native
//!   file dialog (`@tauri-apps/plugin-dialog`, frontend-side) -- `.yaml`/
//!   `.yml` is parsed as a Clash config's `proxies:` list
//!   (`subscription::parse_clash_yaml`), anything else is treated as the
//!   same free-form share-link text `subscription_import_text` accepts.
//!
//! Known simplification (MVP): no dedupe against existing servers. Importing
//! the same subscription/text/file twice will append duplicates rather than
//! merging or skipping already-known servers -- a future pass can dedupe by
//! address+port+protocol (or a subscription-tracked source id) once there's
//! a UI affordance for managing/refreshing a named subscription rather than
//! a one-shot import.

use std::path::Path;

use shared_types::{AppError, AppResult, ServerConfig, UserConfig};
use tauri::{AppHandle, State};

use crate::state::{save_persisted_config, AppState};

/// Shared tail for every import command: reject an empty batch with a
/// user-facing error, otherwise append to the persisted config and return
/// the full, saved `UserConfig`.
async fn import_servers(app: &AppHandle, state: &State<'_, AppState>, servers: Vec<ServerConfig>) -> AppResult<UserConfig> {
    if servers.is_empty() {
        return Err(AppError::new(
            "subscription_empty",
            "no servers could be parsed from this input".to_string(),
        ));
    }

    let mut config = state.config.lock().unwrap();
    config.servers.extend(servers);
    let snapshot = config.clone();
    drop(config);
    save_persisted_config(app, &snapshot).map_err(|e| AppError::new("config_write_failed", e.to_string()))?;
    Ok(snapshot)
}

#[tauri::command]
pub async fn subscription_import(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
) -> AppResult<UserConfig> {
    let body = subscription::fetch_subscription(&url)
        .await
        .map_err(|e| AppError::new("subscription_fetch_failed", e.to_string()))?;

    let (servers, _skipped) = subscription::parse_subscription_body(&body);
    import_servers(&app, &state, servers).await
}

/// Pasted, multi-line raw text containing one or more share-links
/// (`vless://`/`trojan://`/`ss://`/`vmess://`, one per line), optionally
/// whole-body base64-wrapped -- reuses the exact same parsing pipeline as
/// `subscription_import`'s fetched body, just skipping the network fetch.
#[tauri::command]
pub async fn subscription_import_text(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
) -> AppResult<UserConfig> {
    let (servers, _skipped) = subscription::parse_subscription_body(&text);
    import_servers(&app, &state, servers).await
}

/// Imports from a local file the frontend already resolved a path for (via
/// its native open-file dialog). `.yaml`/`.yml` (case-insensitive) is parsed
/// as a Clash config's `proxies:` list; anything else is treated as
/// free-form share-link text, same as `subscription_import_text`.
#[tauri::command]
pub async fn subscription_import_file(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<UserConfig> {
    let contents = std::fs::read_to_string(&path)
        .map_err(|e| AppError::new("subscription_file_read_failed", format!("failed to read '{path}': {e}")))?;

    let is_yaml = Path::new(&path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml"))
        .unwrap_or(false);

    let (servers, _skipped) = if is_yaml {
        subscription::parse_clash_yaml(&contents)
    } else {
        subscription::parse_subscription_body(&contents)
    };

    import_servers(&app, &state, servers).await
}
