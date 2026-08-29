//! Tauri command surface for live connection visibility, backed by
//! `core-manager`'s sing-box Clash API client (`core_manager::clash_api`).
//! Thin delegation only -- see `CoreManager::list_connections`/
//! `close_connection`/`close_all_connections` for the actual behavior
//! (including the `proxy_not_running`/`clash_api_error` `AppError` codes).

use shared_types::{AppResult, ConnectionsSnapshot};
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub async fn connections_list(state: State<'_, AppState>) -> AppResult<ConnectionsSnapshot> {
    state.core_manager.list_connections().await
}

#[tauri::command]
pub async fn connections_close(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.core_manager.close_connection(&id).await
}

#[tauri::command]
pub async fn connections_close_all(state: State<'_, AppState>) -> AppResult<()> {
    state.core_manager.close_all_connections().await
}
