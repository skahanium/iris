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

use crate::ai_runtime::context_cache::ContextAssemblyCache;
use crate::ai_types::{AiScene, AutonomyLevel};
use crate::security::brute_force::BruteForceProtection;

#[derive(Debug, Clone)]
pub struct PendingToolCall {
    pub tool_name: String,
    pub arguments: String,
    pub request_id: String,
    pub scene: AiScene,
    pub note_path: Option<String>,
    pub file_id: Option<i64>,
    pub web_search_enabled: bool,
    pub autonomy_level: AutonomyLevel,
    pub skill_allowed_tools: Vec<String>,
}

// 鈹€鈹€鈹€ Sub-state: Storage 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// Storage infrastructure: database, CAS object store, reference counting,
/// and write guard. Changes to storage internals no longer force recompilation
/// of AI command handlers.
pub struct StorageState {
    pub db: Arc<Database>,
    pub write_guard: WriteGuard,
    cas_store: OnceLock<CasObjectStore>,
    ref_counter: OnceLock<RefCounter>,
}

impl StorageState {
    fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            write_guard: WriteGuard::default(),
            cas_store: OnceLock::new(),
            ref_counter: OnceLock::new(),
        }
    }

    /// Get or initialize the CAS object store (lazy, needs vault path).
    pub fn cas_store(&self, vault: &std::path::Path) -> &CasObjectStore {
        self.cas_store.get_or_init(|| {
            let cas_path = vault.join(".iris").join("cas");
            let store = CasObjectStore::new(cas_path).expect("Failed to create CAS store");
            #[cfg(test)]
            store.enable_encryption([0xC5; 32]);
            #[cfg(not(test))]
            match crate::cas::encryption::get_or_create_cas_key() {
                Ok(key) => store.enable_encryption(key),
                Err(e) => tracing::warn!("CAS encryption unavailable: {e}"),
            }
            store
        })
    }

    pub fn ref_counter(&self) -> &RefCounter {
        self.ref_counter
            .get_or_init(|| RefCounter::new(Arc::clone(&self.db)))
    }
}

// 鈹€鈹€鈹€ Sub-state: AI Runtime 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

/// AI runtime state: pending tool confirmations, active research tasks,
/// embedding queue, and vector index readiness. Changes here don't affect
/// storage-only command handlers.
pub struct AiRuntimeState {
    pub pending_tool_calls: Mutex<HashMap<String, PendingToolCall>>,
    pub active_research: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub context_cache: Mutex<ContextAssemblyCache>,
    pub vector_index_ready: AtomicBool,
    embed_queue: OnceLock<EmbedQueue>,
}

impl AiRuntimeState {
    fn new(vector_ready: bool) -> Self {
        Self {
            pending_tool_calls: Mutex::new(HashMap::new()),
            active_research: Mutex::new(HashMap::new()),
            context_cache: Mutex::new(ContextAssemblyCache::new(64, 30)),
            vector_index_ready: AtomicBool::new(vector_ready),
            embed_queue: OnceLock::new(),
        }
    }

    /// Clear in-memory AI state when switching vaults.
    pub fn clear(&self) {
        if let Ok(mut pending) = self.pending_tool_calls.lock() {
            pending.clear();
        }
        if let Ok(mut research) = self.active_research.lock() {
            research.clear();
        }
        if let Ok(mut cache) = self.context_cache.lock() {
            cache.clear();
        }
        self.vector_index_ready
            .store(false, std::sync::atomic::Ordering::Relaxed);
        tracing::info!(
            "vault switch: cleared pending tool calls, active research, and vector index"
        );
    }
}

// 鈹€鈹€鈹€ AppState (top-level coordinator) 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

pub struct AppState {
    pub storage: StorageState,
    pub ai: AiRuntimeState,
    vault: Mutex<Option<PathBuf>>,
    data_dir: PathBuf,
    pub watcher: Mutex<Option<FileWatcher>>,

    pub db: Arc<Database>,
    pub brute_force: BruteForceProtection,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> AppResult<Arc<Self>> {
        let db_path = data_dir.join("iris.db");
        let db = Arc::new(Database::open(&db_path)?);
        let vector_ready = db.vector_index_ready();

        let storage = StorageState::new(Arc::clone(&db));
        let ai = AiRuntimeState::new(vector_ready);

        let state = Arc::new(Self {
            db: Arc::clone(&storage.db),
            storage,
            ai,
            vault: Mutex::new(None),
            data_dir,
            watcher: Mutex::new(None),
            brute_force: BruteForceProtection::new(),
        });

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
        self.ai
            .vector_index_ready
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    fn ensure_embed_queue(self: &Arc<Self>) -> &EmbedQueue {
        self.ai
            .embed_queue
            .get_or_init(|| EmbedQueue::spawn(Arc::clone(self)))
    }

    pub fn enqueue_embedding(self: &Arc<Self>, file_id: i64) {
        self.ensure_embed_queue().enqueue(file_id);
    }

    /// Get CAS store via the storage sub-state.
    pub fn cas_store(&self) -> AppResult<&CasObjectStore> {
        let vault = self.vault_path()?;
        static EMBEDDING_WARMUP_STARTED: OnceLock<()> = OnceLock::new();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            if EMBEDDING_WARMUP_STARTED.set(()).is_ok() {
                handle.spawn(async {
                    let _ = crate::embedding::engine::embed_text("warmup");
                });
            }
        }
        Ok(self.storage.cas_store(&vault))
    }

    pub fn ref_counter(&self) -> &RefCounter {
        self.storage.ref_counter()
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
            .ok_or_else(|| AppError::msg("绗旇鐩綍鏈厤缃紝璇峰厛閫夋嫨 vault"))
    }

    /// Clear all in-memory AI state when switching vaults.
    pub fn clear_ai_state(&self) {
        self.ai.clear();
    }

    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

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
