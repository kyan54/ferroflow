use std::path::PathBuf;
use std::sync::Mutex;

use core_manager::CoreManager;
use net::SystemProxyManager;
use shared_types::UserConfig;
use tauri::{AppHandle, Manager};

pub struct AppState {
    pub config: Mutex<UserConfig>,
    pub core_manager: CoreManager,
    pub system_proxy: SystemProxyManager,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            config: Mutex::new(UserConfig::default()),
            core_manager: CoreManager::new(),
            system_proxy: SystemProxyManager::new(),
        }
    }
}

fn config_path(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    Some(dir.join("config.json"))
}

/// Best-effort load at startup; a missing/corrupt file just keeps the
/// in-memory default (fresh install) rather than failing app boot.
pub fn load_persisted_config(app: &AppHandle) {
    let Some(path) = config_path(app) else { return };
    let Ok(bytes) = std::fs::read(&path) else { return };
    let Ok(parsed) = serde_json::from_slice::<UserConfig>(&bytes) else {
        tracing::warn!("config.json at {path:?} failed to parse, keeping defaults");
        return;
    };
    let state = app.state::<AppState>();
    *state.config.lock().unwrap() = parsed;
}

pub fn save_persisted_config(app: &AppHandle, config: &UserConfig) -> std::io::Result<()> {
    let Some(path) = config_path(app) else {
        return Err(std::io::Error::other("could not resolve app config dir"));
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(config)?;
    std::fs::write(path, bytes)
}
