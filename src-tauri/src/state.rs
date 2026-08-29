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
    /// The shared secret persisted after a successful `helper_install` on
    /// Windows/macOS (`None` on Linux, where `SO_PEERCRED` is the trust
    /// boundary instead — see `commands::helper_linux`). Mirrored into
    /// `core_manager.set_helper_token` any time it changes, since
    /// `core-manager` needs it to build a `HelperClient` for TUN-mode
    /// start/stop/status but has no Tauri/app-data knowledge of its own.
    pub helper_token: Mutex<Option<String>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            config: Mutex::new(UserConfig::default()),
            core_manager: CoreManager::new(),
            system_proxy: SystemProxyManager::new(),
            helper_token: Mutex::new(None),
        }
    }
}

fn config_path(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    Some(dir.join("config.json"))
}

fn helper_token_path(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    Some(dir.join("helper-token"))
}

/// Best-effort load at startup, mirroring `load_persisted_config`: a
/// missing file (first run, or Linux where none is ever written) just
/// leaves `helper_token`/`core_manager`'s copy at `None`.
pub fn load_persisted_helper_token(app: &AppHandle) {
    let Some(path) = helper_token_path(app) else { return };
    let Ok(token) = std::fs::read_to_string(&path) else { return };
    let token = token.trim().to_string();
    if token.is_empty() {
        return;
    }
    let state = app.state::<AppState>();
    *state.helper_token.lock().unwrap() = Some(token.clone());
    state.core_manager.set_helper_token(Some(token));
}

/// Updates both `AppState.helper_token` and `core_manager`'s copy
/// unconditionally (so a freshly-installed/-uninstalled helper is
/// immediately usable/stopped-being-used for the rest of this run
/// regardless of what happens next), then best-effort persists `token` to
/// disk (or removes the file when `None`, i.e. after `helper_uninstall`)
/// so it survives an app restart. A disk-write failure is returned to the
/// caller to log, but does not undo the in-memory update above.
pub fn set_persisted_helper_token(app: &AppHandle, token: Option<String>) -> std::io::Result<()> {
    let state = app.state::<AppState>();
    *state.helper_token.lock().unwrap() = token.clone();
    state.core_manager.set_helper_token(token.clone());

    let Some(path) = helper_token_path(app) else { return Ok(()) };
    match &token {
        Some(value) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, value)
        }
        None => match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        },
    }
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
