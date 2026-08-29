//! Subscription-link import: fetches a provider's subscription URL, parses
//! whatever share-links it contains (see `subscription::parse` for the
//! supported `vless://`/`trojan://`/`ss://`/`vmess://` formats), appends the
//! newly parsed servers to the persisted config, and returns the updated
//! `UserConfig` -- same save-then-return-clone shape as
//! `commands::config::servers_add`.
//!
//! Known simplification (MVP): no dedupe against existing servers. Importing
//! the same subscription twice will append duplicates rather than merging
//! or skipping already-known servers -- a future pass can dedupe by
//! address+port+protocol (or a subscription-tracked source id) once there's
//! a UI affordance for managing/refreshing a named subscription rather than
//! a one-shot "paste a URL" import.

use shared_types::{AppError, AppResult, UserConfig};
use tauri::{AppHandle, State};

use crate::state::{save_persisted_config, AppState};

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
    if servers.is_empty() {
        return Err(AppError::new(
            "subscription_empty",
            "no servers could be parsed from this subscription".to_string(),
        ));
    }

    let mut config = state.config.lock().unwrap();
    config.servers.extend(servers);
    let snapshot = config.clone();
    drop(config);
    save_persisted_config(&app, &snapshot)
        .map_err(|e| AppError::new("config_write_failed", e.to_string()))?;
    Ok(snapshot)
}
