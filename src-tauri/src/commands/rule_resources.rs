//! Tauri commands for the "Rule resources" feature: curated + custom
//! GeoIP/GeoSite `.srs` rule-set files a `RoutingRule` with
//! `matchType: "ruleSet"` can reference (see
//! `shared_types::RuleMatchType::RuleSet` and
//! `core_manager::config::build_route_rules`/`build_rule_set_entries`).
//! Delegates the actual catalog/URL-building/download mechanics to the
//! `rule-resources` crate; this module is the `UserConfig.rule_resources`
//! state + on-disk storage-path bookkeeping layer on top, mirroring
//! `commands::config`/`commands::rules`'s patterns closely. Also owns the
//! auto-update background task (`spawn_auto_update_task`), started once from
//! `lib.rs`'s `.setup()` hook.

use std::path::PathBuf;
use std::time::Duration;

use shared_types::{AppError, AppResult, RuleResourceCategory, RuleResourceInfo, UserConfig};
use tauri::{AppHandle, Manager, State};

use crate::state::{rule_resources_dir, save_persisted_config, AppState};

/// `shared_types::RuleResourceCategory` and `rule_resources::ResourceCategory`
/// are deliberately distinct types (see the former's doc comment) -- this is
/// the one place both are in scope at once, so the conversion lives here.
fn to_resource_category(category: RuleResourceCategory) -> rule_resources::ResourceCategory {
    match category {
        RuleResourceCategory::Geosite => rule_resources::ResourceCategory::Geosite,
        RuleResourceCategory::GeoIp => rule_resources::ResourceCategory::GeoIp,
    }
}

/// Also used by `commands::proxy::build_resource_paths`, which needs the
/// same naming convention to build the `id -> path` map `CoreManager::start`
/// takes -- kept in one place so the two never drift out of sync.
pub(crate) fn category_file_prefix(category: RuleResourceCategory) -> &'static str {
    match category {
        RuleResourceCategory::Geosite => "geosite",
        RuleResourceCategory::GeoIp => "geoip",
    }
}

/// `<app_config_dir>/rule-resources/<category>-<name>.srs` -- see
/// `state::rule_resources_dir`'s doc comment.
pub(crate) fn resource_file_path(app: &AppHandle, category: RuleResourceCategory, name: &str) -> AppResult<PathBuf> {
    let dir = rule_resources_dir(app).ok_or_else(|| {
        AppError::new("app_data_dir_unavailable", "could not resolve the app config directory")
    })?;
    Ok(dir.join(format!("{}-{}.srs", category_file_prefix(category), name)))
}

#[tauri::command]
pub fn rule_resources_catalog() -> AppResult<Vec<rule_resources::CatalogEntry>> {
    Ok(rule_resources::builtin_catalog())
}

/// Downloads a catalog entry (`name`/`category` must match one of
/// `rule_resources::builtin_catalog()`'s entries -- `rule_resource_not_in_catalog`
/// otherwise) using the *current* `UserConfig.github_accel_prefix`, stores it
/// at its `<category>-<name>.srs` path, and upserts a `RuleResourceInfo` into
/// `UserConfig.rule_resources` (keyed by id -- a re-download of an
/// already-tracked resource just refreshes it in place rather than
/// duplicating the entry).
#[tauri::command]
pub async fn rule_resources_download(
    app: AppHandle,
    state: State<'_, AppState>,
    category: RuleResourceCategory,
    name: String,
) -> AppResult<UserConfig> {
    let resource_category = to_resource_category(category);
    let catalog = rule_resources::builtin_catalog();
    let entry = catalog
        .into_iter()
        .find(|e| e.name == name && e.category == resource_category)
        .ok_or_else(|| {
            AppError::new(
                "rule_resource_not_in_catalog",
                format!("'{name}' is not in the built-in catalog for this category"),
            )
        })?;

    let accel_prefix = state.config.lock().unwrap().github_accel_prefix.clone();
    let url = rule_resources::resource_url(resource_category, &entry.name, accel_prefix.as_deref());
    let dest_path = resource_file_path(&app, category, &entry.name)?;

    let downloaded = rule_resources::download(&url, &dest_path)
        .await
        .map_err(|e| AppError::new("rule_resource_download_failed", e.to_string()))?;

    let info = RuleResourceInfo {
        id: entry.name.clone(),
        name: entry.name,
        category,
        is_builtin: true,
        source_url: url,
        size_bytes: downloaded.size_bytes,
        sha256: downloaded.sha256,
        downloaded_at: downloaded.downloaded_at,
    };

    upsert_resource_and_persist(&app, &state, info)
}

/// Same as `rule_resources_download`, but for an arbitrary user-supplied
/// name/URL rather than a catalog lookup -- the "external"/"custom"
/// download flow for any valid upstream `.srs` filename not in the curated
/// catalog. `url` is used verbatim (already including any acceleration
/// prefix the caller wants, since there's no catalog entry here for
/// `resource_url` to derive one from).
#[tauri::command]
pub async fn rule_resources_download_custom(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    category: RuleResourceCategory,
    url: String,
) -> AppResult<UserConfig> {
    let dest_path = resource_file_path(&app, category, &name)?;

    let downloaded = rule_resources::download(&url, &dest_path)
        .await
        .map_err(|e| AppError::new("rule_resource_download_failed", e.to_string()))?;

    let info = RuleResourceInfo {
        id: name.clone(),
        name,
        category,
        is_builtin: false,
        source_url: url,
        size_bytes: downloaded.size_bytes,
        sha256: downloaded.sha256,
        downloaded_at: downloaded.downloaded_at,
    };

    upsert_resource_and_persist(&app, &state, info)
}

fn upsert_resource_and_persist(
    app: &AppHandle,
    state: &State<AppState>,
    info: RuleResourceInfo,
) -> AppResult<UserConfig> {
    let mut config = state.config.lock().unwrap();
    if let Some(existing) = config.rule_resources.iter_mut().find(|r| r.id == info.id) {
        *existing = info;
    } else {
        config.rule_resources.push(info);
    }
    let snapshot = config.clone();
    drop(config);
    save_persisted_config(app, &snapshot).map_err(|e| AppError::new("config_write_failed", e.to_string()))?;
    Ok(snapshot)
}

/// Re-downloads every currently-tracked resource using its original
/// `category`/`name` (re-derived through `resource_url` with the *current*
/// `github_accel_prefix`, not the stale `source_url` -- the prefix may have
/// changed since the last download). Best-effort per resource: one failing
/// download is logged (`tracing::warn!`) and skipped rather than aborting
/// the rest, so a single unreachable resource doesn't block everything else
/// from updating. Always returns `Ok` with whatever `UserConfig` resulted
/// (successfully-updated resources refreshed, failed ones left as they
/// were) -- this command itself never fails just because some subset of
/// downloads did.
#[tauri::command]
pub async fn rule_resources_update_all(app: AppHandle, state: State<'_, AppState>) -> AppResult<UserConfig> {
    let (accel_prefix, resources) = {
        let config = state.config.lock().unwrap();
        (config.github_accel_prefix.clone(), config.rule_resources.clone())
    };

    let mut failed_ids: Vec<String> = Vec::new();

    for resource in resources {
        let resource_category = to_resource_category(resource.category);
        let url = rule_resources::resource_url(resource_category, &resource.name, accel_prefix.as_deref());
        let dest_path = match resource_file_path(&app, resource.category, &resource.name) {
            Ok(path) => path,
            Err(e) => {
                tracing::warn!("rule-resource auto-update: could not resolve a storage path for '{}': {e}", resource.id);
                failed_ids.push(resource.id);
                continue;
            }
        };

        match rule_resources::download(&url, &dest_path).await {
            Ok(downloaded) => {
                let mut config = state.config.lock().unwrap();
                if let Some(existing) = config.rule_resources.iter_mut().find(|r| r.id == resource.id) {
                    existing.source_url = url;
                    existing.size_bytes = downloaded.size_bytes;
                    existing.sha256 = downloaded.sha256;
                    existing.downloaded_at = downloaded.downloaded_at;
                }
            }
            Err(e) => {
                tracing::warn!("rule-resource auto-update: failed to update '{}': {e}", resource.id);
                failed_ids.push(resource.id);
            }
        }
    }

    if !failed_ids.is_empty() {
        tracing::warn!(
            "rule-resource auto-update: {} of the tracked resources failed to update: {failed_ids:?}",
            failed_ids.len()
        );
    }

    let snapshot = state.config.lock().unwrap().clone();
    save_persisted_config(&app, &snapshot).map_err(|e| AppError::new("config_write_failed", e.to_string()))?;
    Ok(snapshot)
}

/// Removes the file (best-effort -- a missing file is not an error, same
/// convention as `state::set_persisted_helper_token`'s file removal) and the
/// `UserConfig.rule_resources` entry with a matching `id`. A no-op (not an
/// error) if no entry with that id exists, matching `rules_delete`'s
/// forgiving style for an unknown id.
#[tauri::command]
pub fn rule_resources_delete(app: AppHandle, state: State<AppState>, id: String) -> AppResult<UserConfig> {
    let mut config = state.config.lock().unwrap();
    if let Some(pos) = config.rule_resources.iter().position(|r| r.id == id) {
        let resource = config.rule_resources.remove(pos);
        if let Ok(path) = resource_file_path(&app, resource.category, &resource.name) {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => tracing::warn!("rule_resources_delete: failed to remove {}: {e}", path.display()),
            }
        }
    }
    let snapshot = config.clone();
    drop(config);
    save_persisted_config(&app, &snapshot).map_err(|e| AppError::new("config_write_failed", e.to_string()))?;
    Ok(snapshot)
}

/// Background auto-update task, mirroring `core_manager::history::HistoryRecorder`'s
/// `JoinHandle`-based poller shape -- but standalone (not tied to
/// `proxy_start`/`proxy_stop`): rule-resource freshness has nothing to do
/// with whether a proxy run happens to be active, unlike connection
/// history. Started once from `lib.rs`'s `.setup()` hook and left running
/// for the app's whole lifetime (no cancellation handle is kept -- there's
/// nothing that ever needs to stop it early).
///
/// Each iteration sleeps for `UserConfig.rule_resource_auto_update_interval_hours`
/// (re-read fresh every tick, so changing the interval takes effect on the
/// *next* tick without restarting this task), then re-checks
/// `UserConfig.rule_resource_auto_update` (also re-read fresh) before doing
/// anything -- toggling the setting off is honored on the very next
/// wake-up, no restart needed. When on, it runs the exact same
/// re-download-everything logic as `rule_resources_update_all`.
pub fn spawn_auto_update_task(app: AppHandle) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        loop {
            let interval_hours = {
                let state = app.state::<AppState>();
                let config = state.config.lock().unwrap();
                config.rule_resource_auto_update_interval_hours.max(1)
            };
            tokio::time::sleep(Duration::from_secs(u64::from(interval_hours) * 3600)).await;

            let enabled = {
                let state = app.state::<AppState>();
                let config = state.config.lock().unwrap();
                config.rule_resource_auto_update
            };
            if !enabled {
                continue;
            }

            let state = app.state::<AppState>();
            if let Err(e) = rule_resources_update_all(app.clone(), state).await {
                tracing::warn!("rule-resource auto-update tick failed: {e}");
            }
        }
    })
}
