use std::collections::HashMap;

use shared_types::{AppError, AppResult, ProxyStatus};
use tauri::{AppHandle, State};

use crate::state::{rule_resources_dir, AppState};

/// Builds the `id -> .srs file path` map `CoreManager::start` needs to
/// resolve any `RuleMatchType::RuleSet` rule (see
/// `core_manager::config::build_route_rules`) -- one entry per tracked
/// `UserConfig.rule_resources` entry, at the same
/// `<category>-<name>.srs` path `commands::rule_resources` downloads it to.
/// Skips a resource entirely (rather than erroring) if the app config
/// directory can't be resolved -- `core-manager` already treats a missing
/// path for a referenced id as "skip that rule with a warning", so this
/// degrades the same way `RuleForm`'s "resource was deleted after the rule
/// was created" case does.
fn build_resource_paths(app: &AppHandle, state: &State<AppState>) -> HashMap<String, std::path::PathBuf> {
    let Some(dir) = rule_resources_dir(app) else {
        return HashMap::new();
    };
    let config = state.config.lock().unwrap();
    config
        .rule_resources
        .iter()
        .map(|r| {
            let prefix = crate::commands::rule_resources::category_file_prefix(r.category);
            (r.id.clone(), dir.join(format!("{prefix}-{}.srs", r.name)))
        })
        .collect()
}

#[tauri::command]
pub async fn proxy_start(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
) -> AppResult<ProxyStatus> {
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
    let resource_paths = build_resource_paths(&app, &state);
    state.core_manager.start(&server, mode_type, &rules, &resource_paths, connection_history_enabled).await
}

#[tauri::command]
pub async fn proxy_stop(state: State<'_, AppState>) -> AppResult<ProxyStatus> {
    state.core_manager.stop().await
}

#[tauri::command]
pub async fn proxy_status(state: State<'_, AppState>) -> AppResult<ProxyStatus> {
    state.core_manager.status().await
}
