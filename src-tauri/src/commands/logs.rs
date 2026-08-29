//! Tauri commands for reading/clearing the in-memory app+core log ring
//! buffer (`AppState.log_buffer`, a `core_manager::logs::LogBuffer`) -- see
//! that module's doc comment for how it's fed (this app's own `tracing`
//! events via `log_layer::LogCaptureLayer`, and sing-box's child-process
//! stdout/stderr via `core_manager`'s spawned line readers) and
//! `docs/ipc-contract.md`'s "Logs" section for the full picture.

use shared_types::{AppResult, LogEntry};
use tauri::State;

use crate::state::AppState;

/// Every currently-buffered entry, oldest first -- same ordering as
/// `LogBuffer::snapshot`, i.e. a live log view read top-to-bottom, not
/// `history_list`'s most-recent-first look-back convention.
#[tauri::command]
pub async fn logs_get(state: State<'_, AppState>) -> AppResult<Vec<LogEntry>> {
    Ok(state.log_buffer.snapshot())
}

#[tauri::command]
pub async fn logs_clear(state: State<'_, AppState>) -> AppResult<()> {
    state.log_buffer.clear();
    Ok(())
}
