pub mod ai_harness;
pub mod ai_runtime;
pub mod ai_types;
pub mod ai_workflows;
pub mod app;
#[rustfmt::skip]
pub mod crypto;
pub mod cas;
mod chrome_metrics;
mod commands;
mod credentials;
pub mod embedding;
pub mod error;
pub mod indexer;
pub mod knowledge;
mod llm;
mod network;
pub mod recycle;
mod scheduler;
mod security;
pub mod storage;
pub mod version;
mod watcher;
mod window_chrome;

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
            // `AppState::new` 已返回 `Arc<AppState>`；勿再包一层 Arc，否则 Tauri 无法注入 State。
            let state = AppState::new(data_dir)?;
            crate::crypto::vault_key::init_vault_key();
            app.manage(state.clone());

            // Start the scheduler for periodic tasks (GC at 3:00 AM daily)
            // `_scheduler_handle` is intentionally held alive for the app lifetime;
            // dropping it would not stop the spawned task (tokio::spawn detaches).
            let scheduler = scheduler::Scheduler::new(state.clone());
            let _scheduler_handle = scheduler.start();

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
                window_chrome::apply_main_window_chrome(&window);
                window
                    .show()
                    .map_err(|e| crate::error::AppError::msg(format!("无法显示主窗口: {e}")))?;
                // macOS：show 后再应用一次圆角/标题，确保无边框壳层稳定。
                #[cfg(target_os = "macos")]
                window_chrome::apply_main_window_chrome(&window);
                let _ = window.set_focus();
            } else {
                tracing::warn!("main window not found after setup");
            }

            tracing::info!("Iris 已启动 — 若未见窗口，请检查任务栏或 WebView2 运行时。");
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
            commands::file::file_set_lock,
            commands::classified::classified_setup,
            commands::classified::classified_unlock,
            commands::classified::classified_lock,
            commands::classified::classified_status,
            commands::classified::classified_files,
            commands::classified::classified_import,
            commands::classified::classified_export,
            commands::classified::classified_delete,
            commands::classified::classified_mkdir,
            commands::classified::classified_rename,
            commands::file::vault_asset_write,
            commands::file::file_delete,
            commands::file::file_discard,
            commands::file::file_rename,
            commands::file::file_create,
            commands::file::vault_set,
            commands::file::vault_get,
            commands::file::index_rescan,
            commands::file::file_backlinks,
            commands::file::folder_list,
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
            commands::llm_config_commands::llm_config_test_provider,
            commands::llm_config_commands::llm_model_registry_refresh,
            commands::llm_config_commands::llm_model_validate,
            commands::llm_config_commands::llm_model_confirm_capability,
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
            commands::assistant_commands::assistant_execute,
            commands::ai_commands::context_assemble,
            commands::ai_commands::ai_send_message,
            commands::ai_commands::tool_confirm,
            commands::ai_commands::ai_list_tools,
            commands::ai_commands::knowledge_reindex,
            commands::ai_commands::search_hybrid,
            commands::ai_commands::session_list,
            commands::ai_commands::session_delete,
            commands::ai_commands::session_rename,
            commands::ai_commands::session_retract,
            commands::ai_commands::session_load,
            commands::ai_commands::session_clear_all,
            commands::ai_commands::ai_cache_clear,
            commands::ai_commands::harness_resume,
            commands::ai_commands::harness_abort,
            commands::ai_commands::skills_list,
            commands::ai_commands::skills_install,
            commands::ai_commands::skills_uninstall,
            commands::ai_commands::skills_update,
            commands::ai_commands::skills_toggle,
            commands::ai_commands::skills_read,
            commands::ai_commands::skills_write,
            commands::ai_commands::skills_migrate_legacy,
            commands::ai_commands::skills_read_resource,
            commands::ai_commands::tool_audit_query,
            commands::ai_commands::prompt_profile_get,
            commands::ai_commands::prompt_profile_set,
            commands::ai_commands::prompt_profile_presets,
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
            commands::window_chrome_cmd::app_exit,
            commands::window_chrome_cmd::get_desktop_chrome_metrics,
            commands::window_chrome_cmd::reapply_window_chrome,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
