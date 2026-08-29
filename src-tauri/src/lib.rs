mod commands;
mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::config::config_get,
            commands::config::config_save,
            commands::config::servers_add,
            commands::config::servers_delete,
            commands::proxy::proxy_start,
            commands::proxy::proxy_stop,
            commands::proxy::proxy_status,
            commands::connections::connections_list,
            commands::connections::connections_close,
            commands::connections::connections_close_all,
            commands::dashboard::dashboard_open,
            commands::history::history_list,
            commands::history::history_clear,
            commands::rules::rules_add,
            commands::rules::rules_update,
            commands::rules::rules_delete,
            commands::rules::rules_reorder,
            commands::rule_resources::rule_resources_catalog,
            commands::rule_resources::rule_resources_download,
            commands::rule_resources::rule_resources_download_custom,
            commands::rule_resources::rule_resources_update_all,
            commands::rule_resources::rule_resources_delete,
            commands::subscription::subscription_import,
            commands::warp::warp_register,
            commands::system::system_proxy_status,
            commands::system::platform_info,
            commands::helper::helper_get_status,
            commands::helper::helper_install,
            commands::helper::helper_uninstall,
            commands::backup::backup_export,
            commands::backup::backup_import,
            commands::backup::diagnostic_export,
        ])
        .setup(|app| {
            state::load_persisted_config(app.handle());
            state::load_persisted_helper_token(app.handle());
            // Must run before any `proxy_start` could occur -- see
            // `state::init_history_path`'s doc comment.
            state::init_history_path(app.handle());
            // Standalone background task (not tied to proxy start/stop --
            // see its doc comment) that re-downloads tracked rule-set
            // resources on an interval when `rule_resource_auto_update` is
            // on. Started once here, for the app's whole lifetime.
            commands::rule_resources::spawn_auto_update_task(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
