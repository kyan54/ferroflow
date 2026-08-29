use shared_types::{AppError, AppResult, ProxyStatus};
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub async fn proxy_start(state: State<'_, AppState>, server_id: String) -> AppResult<ProxyStatus> {
    let server = {
        let config = state.config.lock().unwrap();
        config.servers.iter().find(|s| s.id == server_id).cloned()
    };
    let Some(server) = server else {
        return Err(AppError::new("server_not_found", format!("no server with id {server_id}")));
    };
    let (mode_type, rules, connection_history_enabled) = {
        let config = state.config.lock().unwrap();
        (config.proxy_mode_type, config.rules.clone(), config.connection_history_enabled)
    };
    state.core_manager.start(&server, mode_type, &rules, connection_history_enabled).await
}

#[tauri::command]
pub async fn proxy_stop(state: State<'_, AppState>) -> AppResult<ProxyStatus> {
    state.core_manager.stop().await
}

#[tauri::command]
pub async fn proxy_status(state: State<'_, AppState>) -> AppResult<ProxyStatus> {
    state.core_manager.status().await
}
