use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;
use tauri::AppHandle;

use crate::cas::ref_counter::RefCounter;
use crate::cas::store::CasObjectStore;
use crate::cas::write_guard::WriteGuard;
use crate::embedding::queue::EmbedQueue;
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;
use crate::watcher::FileWatcher;

/// A tool call awaiting user confirmation.
use crate::ai_runtime::AiScene;

#[derive(Debug, Clone)]
pub struct PendingToolCall {
    pub tool_name: String,
    pub arguments: String,
    pub request_id: String,
    pub scene: AiScene,
    pub note_path: Option<String>,
    pub file_id: Option<i64>,
    pub web_search_enabled: bool,
}

pub struct AppState {
    pub db: Arc<Database>,
    vault: Mutex<Option<PathBuf>>,
    data_dir: PathBuf,
    pub watcher: Mutex<Option<FileWatcher>>,
    /// Active research tasks — keyed by request_id, value is cancel flag
    pub active_research: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Tool calls pending user confirmation — keyed by tool_call_id
    pub pending_tool_calls: Mutex<HashMap<String, PendingToolCall>>,
    /// sqlite-vec vec0 tables available (set at DB open).
    pub vector_index_ready: std::sync::atomic::AtomicBool,
    embed_queue: OnceLock<EmbedQueue>,
    pub write_guard: WriteGuard,
    cas_store: OnceLock<CasObjectStore>,
    ref_counter: OnceLock<RefCounter>,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> AppResult<Arc<Self>> {
        let db_path = data_dir.join("iris.db");
        let db = Arc::new(Database::open(&db_path)?);
        let vector_ready = db.vector_index_ready();

        let state = Arc::new(Self {
            db,
            vault: Mutex::new(None),
            data_dir,
            watcher: Mutex::new(None),
            active_research: Mutex::new(HashMap::new()),
            pending_tool_calls: Mutex::new(HashMap::new()),
            vector_index_ready: std::sync::atomic::AtomicBool::new(vector_ready),
            embed_queue: OnceLock::new(),
            write_guard: WriteGuard::default(),
            cas_store: OnceLock::new(),
            ref_counter: OnceLock::new(),
        });

        // 启动时清理过期缓存
        if let Err(e) = crate::llm::search_web::cleanup_expired_search_cache(&state.db) {
            tracing::warn!("failed to cleanup expired search cache: {e}");
        }
        if let Err(e) = crate::llm::fetch_web_page::cleanup_expired_web_cache(&state.db) {
            tracing::warn!("failed to cleanup expired web cache: {e}");
        }

        if let Some(v) = state.load_vault_setting()? {
            let path = PathBuf::from(v);
            if let Err(e) = state.set_vault(path) {
                tracing::warn!("stored vault_path invalid, cleared: {e}");
                state.clear_vault_setting()?;
            }
        }
        Ok(state)
    }

    pub fn is_vector_index_ready(&self) -> bool {
        self.vector_index_ready
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    fn ensure_embed_queue(self: &Arc<Self>) -> &EmbedQueue {
        self.embed_queue
            .get_or_init(|| EmbedQueue::spawn(Arc::clone(self)))
    }

    /// Queue background embedding for a file after index metadata is written.
    pub fn enqueue_embedding(self: &Arc<Self>, file_id: i64) {
        self.ensure_embed_queue().enqueue(file_id);
    }

    /// 获取 CAS 存储实例
    pub fn cas_store(&self) -> &CasObjectStore {
        self.cas_store.get_or_init(|| {
            let vault = self.vault_path().expect("Vault not configured");
            let cas_path = vault.join(".iris").join("cas");
            let store = CasObjectStore::new(cas_path).expect("Failed to create CAS store");
            match crate::cas::encryption::get_or_create_cas_key() {
                Ok(key) => store.enable_encryption(key),
                Err(e) => tracing::warn!("CAS encryption unavailable: {e}"),
            }
            store
        })
    }

    /// 获取引用计数管理器实例
    pub fn ref_counter(&self) -> &RefCounter {
        self.ref_counter
            .get_or_init(|| RefCounter::new(Arc::clone(&self.db)))
    }

    fn clear_vault_setting(&self) -> AppResult<()> {
        {
            let mut guard = self.vault.lock().map_err(|_| AppError::msg("Lock error"))?;
            *guard = None;
        }
        self.db.with_conn(|conn| {
            conn.execute("DELETE FROM settings WHERE key = 'vault_path'", [])?;
            Ok(())
        })
    }

    fn load_vault_setting(&self) -> AppResult<Option<String>> {
        self.db.with_conn(|conn| {
            let result: Result<String, _> = conn.query_row(
                "SELECT value FROM settings WHERE key = 'vault_path'",
                [],
                |r| r.get(0),
            );
            match result {
                Ok(json) => {
                    let v: Value = serde_json::from_str(&json)?;
                    Ok(v.as_str().map(|s| s.to_string()))
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
    }

    pub fn set_vault(&self, path: PathBuf) -> AppResult<()> {
        if !path.is_dir() {
            return Err(AppError::msg("Vault must be a directory"));
        }
        let canonical = path.canonicalize().unwrap_or_else(|e| {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "vault canonicalize failed; using path as selected"
            );
            path
        });
        {
            let mut guard = self.vault.lock().map_err(|_| AppError::msg("Lock error"))?;
            *guard = Some(canonical.clone());
        }
        let json = serde_json::to_string(canonical.to_string_lossy().as_ref())?;
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('vault_path', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [json],
            )?;
            Ok(())
        })
    }

    pub fn vault_path(&self) -> AppResult<PathBuf> {
        let guard = self.vault.lock().map_err(|_| AppError::msg("Lock error"))?;
        guard
            .clone()
            .ok_or_else(|| AppError::msg("笔记目录未配置，请先选择 vault"))
    }

    /// Clear in-memory AI state when switching vaults to prevent data leakage
    /// between unrelated note collections.
    pub fn clear_ai_state(&self) {
        if let Ok(mut pending) = self.pending_tool_calls.lock() {
            pending.clear();
        }
        if let Ok(mut research) = self.active_research.lock() {
            research.clear();
        }
        tracing::info!("vault switch: cleared pending tool calls and active research");
    }

    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

    /// 停止旧监听并在当前 vault 上启动新的 `FileWatcher`（`vault_set` 后调用）。
    pub fn restart_file_watcher(self: &Arc<Self>, app: AppHandle) -> AppResult<()> {
        {
            let mut guard = self
                .watcher
                .lock()
                .map_err(|_| AppError::msg("Lock error"))?;
            *guard = None;
        }

        let watcher = FileWatcher::start(app, self.clone())?;
        let mut guard = self
            .watcher
            .lock()
            .map_err(|_| AppError::msg("Lock error"))?;
        *guard = Some(watcher);
        Ok(())
    }
}
