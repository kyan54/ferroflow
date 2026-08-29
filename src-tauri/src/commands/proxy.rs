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
    state.core_manager.start(&server).await
}

#[tauri::command]
pub async fn proxy_stop(state: State<'_, AppState>) -> AppResult<ProxyStatus> {
    state.core_manager.stop().await
}

#[tauri::command]
pub async fn proxy_status(state: State<'_, AppState>) -> AppResult<ProxyStatus> {
    state.core_manager.status().await
}
