//! Tauri commands for reading/clearing the local connection-history log --
//! plain delegation to file I/O at the path `state::history_path` picks
//! (`<app_config_dir>/connection-history.jsonl`), written by
//! `core_manager::history::HistoryRecorder` in the background whenever
//! `UserConfig.connection_history_enabled` is `true`. See
//! `docs/ipc-contract.md`'s "Connection history" section for the full
//! picture (opt-in default, 1000-line cap, no encryption).

use shared_types::{AppError, AppResult, HistoryEntry};
use tauri::AppHandle;

use crate::state::history_path;

/// Reads every entry from the history file and returns them most-recent-first
/// (the file itself is oldest-first, one JSON line appended per finished
/// connection). A missing file -- fresh install, or history has simply never
/// been enabled -- returns an empty list rather than an error; a line that
/// fails to parse (shouldn't happen absent manual file tampering or a format
/// change) is skipped rather than failing the whole read.
#[tauri::command]
pub async fn history_list(app: AppHandle) -> AppResult<Vec<HistoryEntry>> {
    let Some(path) = history_path(&app) else {
        return Ok(Vec::new());
    };

    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(AppError::new(
                "history_read_failed",
                format!("failed to read connection history: {e}"),
            ))
        }
    };

    let mut entries: Vec<HistoryEntry> = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    entries.reverse();
    Ok(entries)
}

/// Deletes the history file. Idempotent -- a missing file is not an error,
/// same convention as `state::set_persisted_helper_token`'s file removal.
#[tauri::command]
pub async fn history_clear(app: AppHandle) -> AppResult<()> {
    let Some(path) = history_path(&app) else {
        return Ok(());
    };

    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AppError::new(
            "history_clear_failed",
            format!("failed to clear connection history: {e}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    // `history_list`/`history_clear` both need a `tauri::AppHandle` (for
    // `app_config_dir()`), which isn't constructible without a running Tauri
    // app -- no unit tests here for the same reason `commands::connections`
    // has none; the parsing/reversal/error-mapping logic they wrap is thin
    // enough that it's covered indirectly by `core_manager::history`'s own
    // tests (`HistoryEntry` round-tripping, the on-disk line format) instead.
}
