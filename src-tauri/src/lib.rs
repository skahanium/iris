pub mod ai_runtime;
pub mod app;
mod commands;
mod credentials;
pub mod embedding;
pub mod error;
pub mod indexer;
pub mod knowledge;
mod llm;
mod network;
pub mod recycle;
mod security;
pub mod storage;
pub mod version;
mod watcher;
mod window_chrome;

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
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| crate::error::AppError::msg(format!("无法解析应用数据目录: {e}")))?;
            std::fs::create_dir_all(&data_dir)?;
            let state = Arc::new(AppState::new(data_dir)?);
            app.manage(state.clone());

            // Clean up stale version snapshots and expired recycle bin on startup
            let _ = crate::version::version_cleanup(&state);
            let _ = crate::recycle::purge_expired(&state);

            if state.vault_path().is_ok() {
                let _ = state.restart_file_watcher(app.handle().clone());
            }

            if let Some(window) = app.get_webview_window("main") {
                #[cfg(windows)]
                {
                    use tauri::Theme;
                    let _ = window.set_theme(Some(Theme::Dark));
                }
                window
                    .show()
                    .map_err(|e| crate::error::AppError::msg(format!("无法显示主窗口: {e}")))?;
                // macOS 在 show 后再应用 effect，部分系统版本上更稳定
                window_chrome::apply_main_window_chrome(&window);
                let _ = window.set_focus();
            } else {
                tracing::warn!("main window not found after setup");
            }

            eprintln!("Iris 已启动 — 若未见窗口，请检查任务栏或 WebView2 运行时。");
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
            commands::file::file_discard,
            commands::file::file_rename,
            commands::file::file_create,
            commands::file::vault_set,
            commands::file::vault_get,
            commands::file::index_rescan,
            commands::file::file_backlinks,
            commands::file::folder_create,
            commands::file::folder_rename,
            commands::file::folder_delete,
            commands::recycle::recycle_list_cmd,
            commands::recycle::recycle_restore_cmd,
            commands::recycle::recycle_purge_cmd,
            commands::search::search_keyword,
            commands::search::search_semantic,
            commands::search::search_reindex,
            commands::llm::llm_providers,
            commands::llm::llm_generate,
            commands::llm::llm_chat,
            commands::llm::llm_abort_cmd,
            commands::llm_config_commands::llm_config_get,
            commands::llm_config_commands::llm_config_set,
            commands::llm_config_commands::llm_config_apply_deepseek_defaults,
            commands::llm_config_commands::connectivity_status,
            commands::llm_config_commands::llm_config_test,
            commands::minimax_config_commands::minimax_config_get,
            commands::minimax_config_commands::minimax_config_set,
            commands::minimax_config_commands::minimax_config_test,
            commands::graph::graph_data,
            commands::version::version_list_cmd,
            commands::version::version_preview_cmd,
            commands::version::version_restore_cmd,
            commands::version::version_delete_cmd,
            commands::version::version_finalize_current_cmd,
            commands::version::version_cleanup_cmd,
            commands::version::version_save_manual_cmd,
            commands::version::version_save_idle_cmd,
            commands::template::template_list,
            commands::template::template_create,
            commands::template::template_read,
            commands::template::template_save,
            commands::template::template_delete,
            commands::tag::tag_list,
            commands::export::export_file,
            commands::corpus_commands::corpus_list,
            commands::corpus_commands::corpus_upsert,
            // Writing Workflow (Phase 1)
            commands::writing_commands::writing_execute,
            commands::writing_commands::patch_apply,
            commands::file::path_sync_suggest,
            // Citation Check Workflow (Phase 1)
            commands::citation_commands::citation_check,
            // Organize Workflow (Phase 2)
            commands::organize_commands::organize_execute,
            commands::organize_commands::organize_apply,
            // Chapter & Document Writing (Phase 3)
            commands::document_commands::chapter_writing_execute,
            commands::document_commands::document_check_execute,
            commands::document_commands::parse_document_chapters,
            commands::ai_commands::context_assemble,
            commands::ai_commands::ai_send_message,
            commands::ai_commands::tool_confirm,
            commands::ai_commands::ai_list_tools,
            commands::ai_commands::knowledge_reindex,
            commands::ai_commands::search_hybrid,
            // Research Workflow (D)
            commands::research_commands::research_execute,
            commands::research_commands::research_status,
            commands::research_commands::research_abort,
            commands::research_commands::research_active_tasks,
            commands::research_commands::research_generate_note,
            // Personalization (E)
            commands::profile_commands::profile_list,
            commands::profile_commands::profile_get,
            commands::profile_commands::profile_set,
            commands::profile_commands::profile_set_rule,
            commands::profile_commands::profile_deactivate,
            commands::profile_commands::profile_delete,
            commands::profile_commands::inbox_list,
            commands::profile_commands::inbox_add,
            commands::profile_commands::inbox_update_status,
            commands::profile_commands::inbox_delete,
            commands::profile_commands::inbox_counts,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
