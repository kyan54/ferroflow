use shared_types::{AppResult, HelperPlatform, PlatformInfo, SystemProxyStatus};
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub fn system_proxy_status(state: State<AppState>) -> AppResult<SystemProxyStatus> {
    state.system_proxy.status()
}

#[tauri::command]
pub fn platform_info() -> AppResult<PlatformInfo> {
    let platform = if cfg!(target_os = "windows") {
        HelperPlatform::Windows
    } else if cfg!(target_os = "macos") {
        HelperPlatform::Macos
    } else {
        HelperPlatform::Linux
    };

    Ok(PlatformInfo {
        platform,
        arch: std::env::consts::ARCH.to_string(),
        os_version: String::new(),
        is_admin: false,
    })
}
