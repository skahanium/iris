use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
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
use crate::ai_types::{AiScene, AutonomyLevel, SkillActivationPlanSummary};
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
    pub skill_activation_plan: Option<SkillActivationPlanSummary>,
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
    cas_key_override: Option<[u8; 32]>,
}

impl StorageState {
    fn new(db: Arc<Database>, cas_key_override: Option<[u8; 32]>) -> Self {
        Self {
            db,
            write_guard: WriteGuard::default(),
            cas_store: OnceLock::new(),
            ref_counter: OnceLock::new(),
            cas_key_override,
        }
    }

    /// Get or initialize the CAS object store (lazy, needs vault path).
    pub fn cas_store(&self, vault: &std::path::Path) -> AppResult<&CasObjectStore> {
        if let Some(store) = self.cas_store.get() {
            return Ok(store);
        }

        let cas_path = vault.join(".iris").join("cas");
        let store = CasObjectStore::new(cas_path)?;
        if let Some(key) = self.cas_key_override {
            store.enable_encryption(key);
        } else {
            #[cfg(test)]
            store.enable_encryption([0xC5; 32]);
            #[cfg(not(test))]
            {
                let key = crate::cas::encryption::get_or_create_cas_key().map_err(|e| {
                    AppError::msg(format!(
                        "CAS encryption unavailable; refusing plaintext writes: {e}"
                    ))
                })?;
                store.enable_encryption(key);
            }
        }
        let _ = self.cas_store.set(store);
        self.cas_store
            .get()
            .ok_or_else(|| AppError::msg("Failed to initialize CAS store"))
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

pub struct DocumentOpenState {
    active_tokens: Mutex<HashSet<String>>,
    next_token: AtomicU64,
}

impl DocumentOpenState {
    fn new() -> Self {
        Self {
            active_tokens: Mutex::new(HashSet::new()),
            next_token: AtomicU64::new(1),
        }
    }

    fn begin(&self) -> String {
        let token = format!(
            "doc-open-{}",
            self.next_token
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        if let Ok(mut active) = self.active_tokens.lock() {
            active.insert(token.clone());
        }
        token
    }

    fn end(&self, token: &str) -> bool {
        self.active_tokens
            .lock()
            .map(|mut active| active.remove(token))
            .unwrap_or(false)
    }

    fn count(&self) -> usize {
        self.active_tokens
            .lock()
            .map(|active| active.len())
            .unwrap_or(0)
    }
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
        crate::llm::safe_lock(&self.context_cache).clear();
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
    pub document_open: DocumentOpenState,
    vault: Mutex<Option<PathBuf>>,
    data_dir: PathBuf,
    pub watcher: Mutex<Option<FileWatcher>>,

    pub db: Arc<Database>,
    pub brute_force: BruteForceProtection,
}

impl AppState {
    /// Create application state using the production CAS key source.
    pub fn new(data_dir: PathBuf) -> AppResult<Arc<Self>> {
        Self::new_with_cas_key_override(data_dir, None)
    }

    /// Create application state with a deterministic CAS key for integration tests.
    #[doc(hidden)]
    pub fn new_with_test_cas_key(data_dir: PathBuf, cas_key: [u8; 32]) -> AppResult<Arc<Self>> {
        Self::new_with_cas_key_override(data_dir, Some(cas_key))
    }

    fn new_with_cas_key_override(
        data_dir: PathBuf,
        cas_key_override: Option<[u8; 32]>,
    ) -> AppResult<Arc<Self>> {
        let db_path = data_dir.join("iris.db");
        let db = Arc::new(Database::open(&db_path)?);
        let vector_ready = db.vector_index_ready();

        let storage = StorageState::new(Arc::clone(&db), cas_key_override);
        let ai = AiRuntimeState::new(vector_ready);

        let state = Arc::new(Self {
            db: Arc::clone(&storage.db),
            storage,
            ai,
            document_open: DocumentOpenState::new(),
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

    pub fn begin_document_open(&self) -> String {
        self.document_open.begin()
    }

    pub fn end_document_open(&self, token: &str) -> bool {
        self.document_open.end(token)
    }

    pub fn foreground_document_open_count(&self) -> usize {
        self.document_open.count()
    }

    pub fn has_foreground_document_open(&self) -> bool {
        self.foreground_document_open_count() > 0
    }

    /// Get CAS store via the storage sub-state.
    pub fn cas_store(&self) -> AppResult<&CasObjectStore> {
        let vault = self.vault_path()?;
        self.storage.cas_store(&vault)
    }

    pub fn ref_counter(&self) -> &RefCounter {
        self.storage.ref_counter()
    }

    fn clear_vault_setting(&self) -> AppResult<()> {
        crate::ai_runtime::agent_task::AgentTaskRuntime::abort_recoverable_tasks(
            &self.db,
            "VAULT_RESET",
            "Vault reset invalidated recoverable task state",
        )?;
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

    /// Clear context assembly cache (called on .md file changes to prevent stale results).
    pub fn clear_context_cache(&self) {
        crate::llm::safe_lock(&self.ai.context_cache).clear();
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

#[cfg(test)]
mod document_open_state_tests {
    use super::*;

    #[test]
    fn document_open_tokens_are_counted_and_duplicate_end_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::new_with_test_cas_key(dir.path().join("data"), [0xA5; 32]).unwrap();

        assert_eq!(state.foreground_document_open_count(), 0);
        let first = state.begin_document_open();
        let second = state.begin_document_open();
        assert_ne!(first, second);
        assert_eq!(state.foreground_document_open_count(), 2);

        assert!(state.end_document_open(&first));
        assert_eq!(state.foreground_document_open_count(), 1);
        assert!(!state.end_document_open(&first));
        assert_eq!(state.foreground_document_open_count(), 1);
        assert!(state.end_document_open(&second));
        assert_eq!(state.foreground_document_open_count(), 0);
    }
}
