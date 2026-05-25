pub mod app;
mod commands;
mod credentials;
pub mod embedding;
pub mod error;
pub mod indexer;
mod llm;
pub mod storage;
pub mod version;
mod watcher;

use std::sync::Arc;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

use app::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir().expect("app data dir");
            std::fs::create_dir_all(&data_dir)?;
            let state = Arc::new(AppState::new(data_dir)?);
            app.manage(state.clone());

            // Clean up stale version snapshots on startup
            let _ = crate::version::version_cleanup(&state);

            if state.vault_path().is_ok() {
                let _ = state.restart_file_watcher(app.handle().clone());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::settings::settings_reset,
            commands::settings::credential_set,
            commands::settings::credential_has,
            commands::settings::credential_delete,
            commands::file::file_list,
            commands::file::file_read,
            commands::file::file_write,
            commands::file::file_delete,
            commands::file::file_rename,
            commands::file::file_create,
            commands::file::vault_set,
            commands::file::vault_get,
            commands::file::index_rescan,
            commands::file::file_backlinks,
            commands::search::search_keyword,
            commands::search::search_semantic,
            commands::search::search_reindex,
            commands::llm::llm_providers,
            commands::llm::llm_generate,
            commands::llm::llm_chat,
            commands::llm::llm_abort_cmd,
            commands::graph::graph_data,
            commands::version::version_list_cmd,
            commands::version::version_preview_cmd,
            commands::version::version_restore_cmd,
            commands::version::version_delete_cmd,
            commands::version::version_finalize_cmd,
            commands::version::version_cleanup_cmd,
            commands::template::template_list,
            commands::template::template_create,
            commands::tag::tag_list,
            commands::export::export_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
