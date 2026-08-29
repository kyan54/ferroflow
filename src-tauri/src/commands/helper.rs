//! Platform-agnostic Tauri command surface for the privileged helper.
//! Dispatches to whichever of `helper_windows`/`helper_macos`/
//! `helper_linux` matches this build's target OS -- each exposes the same
//! `get_status(token)`/`install(app)`/`uninstall()` shape (see
//! `docs/ipc-contract.md`'s "Helper install flow" section), so this file
//! only has to plug in the bits that differ from platform to platform
//! (token persistence, pushing the token into `core_manager`).

use shared_types::{AppResult, HelperStatus};
use tauri::{AppHandle, State};

use crate::state::{self, AppState};

#[cfg(target_os = "windows")]
use super::helper_windows as platform;
#[cfg(target_os = "macos")]
use super::helper_macos as platform;
#[cfg(target_os = "linux")]
use super::helper_linux as platform;

#[tauri::command]
pub async fn helper_get_status(state: State<'_, AppState>) -> AppResult<HelperStatus> {
    let token = state.helper_token.lock().unwrap().clone();
    platform::get_status(token).await
}

#[tauri::command]
pub async fn helper_install(app: AppHandle) -> AppResult<HelperStatus> {
    let (status, token) = platform::install(&app).await?;
    // Best-effort: a failure to persist the token to disk shouldn't hide a
    // successful install from the caller (the token still lives in
    // `AppState` for this session either way, via `set_persisted_helper_token`
    // updating it before the write) -- just log it.
    if let Err(err) = state::set_persisted_helper_token(&app, token) {
        tracing::warn!("failed to persist helper token: {err}");
    }
    Ok(status)
}

#[tauri::command]
pub async fn helper_uninstall(app: AppHandle) -> AppResult<HelperStatus> {
    let status = platform::uninstall().await?;
    if let Err(err) = state::set_persisted_helper_token(&app, None) {
        tracing::warn!("failed to remove persisted helper token: {err}");
    }
    Ok(status)
}
