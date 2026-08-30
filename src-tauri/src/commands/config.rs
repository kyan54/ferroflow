use shared_types::{AppError, AppResult, RegionRoutingConfig, ServerConfig, UserConfig};
use tauri::{AppHandle, State};

use crate::state::{save_persisted_config, AppState};

#[tauri::command]
pub fn config_get(state: State<AppState>) -> AppResult<UserConfig> {
    Ok(state.config.lock().unwrap().clone())
}

#[tauri::command]
pub fn config_save(app: AppHandle, state: State<AppState>, config: UserConfig) -> AppResult<()> {
    *state.config.lock().unwrap() = config.clone();
    save_persisted_config(&app, &config)
        .map_err(|e| AppError::new("config_write_failed", e.to_string()))
}

#[tauri::command]
pub fn servers_add(app: AppHandle, state: State<AppState>, server: ServerConfig) -> AppResult<UserConfig> {
    let mut config = state.config.lock().unwrap();
    config.servers.push(server);
    let snapshot = config.clone();
    drop(config);
    save_persisted_config(&app, &snapshot)
        .map_err(|e| AppError::new("config_write_failed", e.to_string()))?;
    Ok(snapshot)
}

/// Replaces the server matching `server.id` in place (a no-op if the id
/// isn't found, matching `rules_update`'s exact same lenient shape) --
/// preserves the server's position in the list, unlike a
/// delete-then-add round trip from the frontend would.
#[tauri::command]
pub fn servers_update(app: AppHandle, state: State<AppState>, server: ServerConfig) -> AppResult<UserConfig> {
    let mut config = state.config.lock().unwrap();
    if let Some(existing) = config.servers.iter_mut().find(|s| s.id == server.id) {
        *existing = server;
    }
    let snapshot = config.clone();
    drop(config);
    save_persisted_config(&app, &snapshot)
        .map_err(|e| AppError::new("config_write_failed", e.to_string()))?;
    Ok(snapshot)
}

/// Replaces `UserConfig.region_routing` wholesale with `region_routing` and
/// persists it -- the frontend always sends the full merged struct (it reads
/// the current value, merges its own patch client-side, then calls this),
/// same convention as `servers_update`.
#[tauri::command]
pub fn region_routing_update(
    app: AppHandle,
    state: State<AppState>,
    region_routing: RegionRoutingConfig,
) -> AppResult<UserConfig> {
    let mut config = state.config.lock().unwrap();
    config.region_routing = region_routing;
    let snapshot = config.clone();
    drop(config);
    save_persisted_config(&app, &snapshot)
        .map_err(|e| AppError::new("config_write_failed", e.to_string()))?;
    Ok(snapshot)
}

#[tauri::command]
pub fn servers_delete(app: AppHandle, state: State<AppState>, id: String) -> AppResult<UserConfig> {
    let mut config = state.config.lock().unwrap();
    config.servers.retain(|s| s.id != id);
    if config.selected_server_id.as_deref() == Some(id.as_str()) {
        config.selected_server_id = None;
    }
    let snapshot = config.clone();
    drop(config);
    save_persisted_config(&app, &snapshot)
        .map_err(|e| AppError::new("config_write_failed", e.to_string()))?;
    Ok(snapshot)
}
