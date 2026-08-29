use shared_types::{AppError, AppResult, RoutingRule, UserConfig};
use tauri::{AppHandle, State};

use crate::state::{save_persisted_config, AppState};

#[tauri::command]
pub fn rules_add(app: AppHandle, state: State<AppState>, rule: RoutingRule) -> AppResult<UserConfig> {
    let mut config = state.config.lock().unwrap();
    config.rules.push(rule);
    let snapshot = config.clone();
    drop(config);
    save_persisted_config(&app, &snapshot)
        .map_err(|e| AppError::new("config_write_failed", e.to_string()))?;
    Ok(snapshot)
}

/// Replaces the rule with a matching `id`. If no rule with that id exists,
/// this is a no-op (returns the current config unchanged) rather than an
/// error, matching this codebase's forgiving style elsewhere (e.g.
/// `servers_delete` on an unknown id).
#[tauri::command]
pub fn rules_update(app: AppHandle, state: State<AppState>, rule: RoutingRule) -> AppResult<UserConfig> {
    let mut config = state.config.lock().unwrap();
    if let Some(existing) = config.rules.iter_mut().find(|r| r.id == rule.id) {
        *existing = rule;
    }
    let snapshot = config.clone();
    drop(config);
    save_persisted_config(&app, &snapshot)
        .map_err(|e| AppError::new("config_write_failed", e.to_string()))?;
    Ok(snapshot)
}

#[tauri::command]
pub fn rules_delete(app: AppHandle, state: State<AppState>, id: String) -> AppResult<UserConfig> {
    let mut config = state.config.lock().unwrap();
    config.rules.retain(|r| r.id != id);
    let snapshot = config.clone();
    drop(config);
    save_persisted_config(&app, &snapshot)
        .map_err(|e| AppError::new("config_write_failed", e.to_string()))?;
    Ok(snapshot)
}

/// Re-sorts `config.rules` to match the order of ids in `ordered_ids`. Any
/// id in `ordered_ids` not present in the running config is ignored; any
/// existing rule whose id isn't in `ordered_ids` is dropped to the end,
/// keeping its relative order — this is meant to be called with the full
/// current id list in a new order (e.g. after an up/down move), not a
/// partial reorder.
#[tauri::command]
pub fn rules_reorder(
    app: AppHandle,
    state: State<AppState>,
    ordered_ids: Vec<String>,
) -> AppResult<UserConfig> {
    let mut config = state.config.lock().unwrap();

    let mut reordered: Vec<RoutingRule> = Vec::with_capacity(config.rules.len());
    for id in &ordered_ids {
        if let Some(pos) = config.rules.iter().position(|r| &r.id == id) {
            reordered.push(config.rules.remove(pos));
        }
    }
    // Anything left in `config.rules` had an id not present in `ordered_ids`
    // — keep it, appended after the reordered ones, rather than silently
    // dropping it.
    reordered.append(&mut config.rules);
    config.rules = reordered;

    let snapshot = config.clone();
    drop(config);
    save_persisted_config(&app, &snapshot)
        .map_err(|e| AppError::new("config_write_failed", e.to_string()))?;
    Ok(snapshot)
}
