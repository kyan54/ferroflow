//! Tauri command for streaming/AI-service "unlock" checks -- see
//! `core_manager::unlock` for the actual probing logic and
//! `CoreManager::check_unlock` for the "proxy not running"/"no local port"
//! guards. Thin delegation only.

use shared_types::{AppResult, UnlockResult};
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub async fn unlock_check(state: State<'_, AppState>) -> AppResult<Vec<UnlockResult>> {
    state.core_manager.check_unlock().await
}
