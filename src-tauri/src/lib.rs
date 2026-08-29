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
            commands::subscription::subscription_import,
            commands::system::system_proxy_status,
            commands::system::platform_info,
            commands::helper::helper_get_status,
            commands::helper::helper_install,
            commands::helper::helper_uninstall,
        ])
        .setup(|app| {
            state::load_persisted_config(app.handle());
            state::load_persisted_helper_token(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
