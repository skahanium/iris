use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;
use tauri::AppHandle;

use crate::cas::ref_counter::RefCounter;
use crate::cas::store::CasObjectStore;
use crate::cas::write_guard::WriteGuard;
use crate::embedding::scheduler::{recover_interrupted_generation, EmbeddingScheduler};
use crate::error::{AppError, AppResult};
use crate::feed::fetch::ProdNetGate;
use crate::feed::fulltext::FeedFulltextService;
use crate::feed::repository::FeedRepository;
use crate::feed::sync::FeedSyncService;
use crate::paths::IrisPaths;
use crate::storage::db::Database;
use crate::watcher::FileWatcher;

use crate::ai_runtime::skills::{ActivationIndexMap, SkillEntry};
use crate::ai_types::{AutonomyLevel, SkillActivationPlanSummary};
use crate::security::brute_force::BruteForceProtection;

const PENDING_TOOL_CALL_TTL: Duration = Duration::from_secs(30 * 60);
const MAX_PENDING_TOOL_CALLS: usize = 128;

#[derive(Debug, Clone)]
pub struct PendingToolCall {
    pub tool_name: String,
    pub arguments: String,
    pub request_id: String,
    /// Owning session used for any Session-scoped permission grant.
    pub session_id: i64,
    pub note_path: Option<String>,
    pub file_id: Option<i64>,
    pub web_search_enabled: bool,
    pub autonomy_level: AutonomyLevel,
    pub depth: u32,
    pub skill_activation_plan: Option<SkillActivationPlanSummary>,
    pub created_at: Instant,
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
                let ring = crate::cas::encryption::load_or_create_cas_ring().map_err(|e| {
                    AppError::msg(format!(
                        "CAS encryption unavailable; refusing plaintext writes: {e}"
                    ))
                })?;
                store.enable_encryption_ring(ring);
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
    pub(crate) classified_ephemeral:
        Mutex<crate::ai_runtime::classified_ephemeral::ClassifiedEphemeralStore>,
    pub vector_index_ready: AtomicBool,
    /// Fully parsed, user-confirmed Skills keyed by the currently selected vault.
    ///
    /// Normal Runs may only read this in-memory registry. Filesystem scanning is
    /// confined to vault activation and explicit user refresh operations.
    skill_registry: Mutex<HashMap<PathBuf, Vec<SkillEntry>>>,
    embedding_scheduler: OnceLock<Arc<EmbeddingScheduler>>,
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
            classified_ephemeral: Mutex::new(
                crate::ai_runtime::classified_ephemeral::ClassifiedEphemeralStore::default(),
            ),
            vector_index_ready: AtomicBool::new(vector_ready),
            skill_registry: Mutex::new(HashMap::new()),
            embedding_scheduler: OnceLock::new(),
        }
    }

    pub fn expire_pending_tool_calls(&self) -> Vec<(String, PendingToolCall)> {
        if let Ok(mut pending) = self.pending_tool_calls.lock() {
            return Self::expire_pending_tool_calls_locked(&mut pending, Instant::now());
        }
        Vec::new()
    }

    fn expire_pending_tool_calls_locked(
        pending: &mut HashMap<String, PendingToolCall>,
        now: Instant,
    ) -> Vec<(String, PendingToolCall)> {
        let expired_ids = pending
            .iter()
            .filter(|(_, call)| now.duration_since(call.created_at) > PENDING_TOOL_CALL_TTL)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        expired_ids
            .into_iter()
            .filter_map(|id| pending.remove(&id).map(|call| (id, call)))
            .collect()
    }
    pub fn prune_pending_tool_calls(&self) {
        if let Ok(mut pending) = self.pending_tool_calls.lock() {
            Self::prune_pending_tool_calls_locked(&mut pending, Instant::now());
        }
    }

    fn replace_skill_activation_snapshot(
        &self,
        scheduler: &EmbeddingScheduler,
        vault: PathBuf,
        skills: Vec<SkillEntry>,
        activation_index: ActivationIndexMap,
    ) -> AppResult<()> {
        let mut registry = self
            .skill_registry
            .lock()
            .map_err(|_| AppError::msg("skill_registry_lock_failed"))?;
        scheduler.replace_skill_activation_index(activation_index)?;
        registry.clear();
        registry.insert(vault, skills);
        Ok(())
    }

    fn cached_skills(&self, vault: &std::path::Path) -> AppResult<Option<Vec<SkillEntry>>> {
        let registry = self
            .skill_registry
            .lock()
            .map_err(|_| AppError::msg("skill_registry_lock_failed"))?;
        Ok(registry.get(vault).cloned())
    }

    fn cached_skill_activation(
        &self,
        scheduler: &EmbeddingScheduler,
        vault: &std::path::Path,
    ) -> AppResult<Option<(Vec<SkillEntry>, ActivationIndexMap)>> {
        let registry = self
            .skill_registry
            .lock()
            .map_err(|_| AppError::msg("skill_registry_lock_failed"))?;
        let Some(skills) = registry.get(vault) else {
            return Ok(None);
        };
        Ok(Some((
            skills.clone(),
            scheduler.cached_skill_activation_index(),
        )))
    }

    fn skills_with_upsert(
        &self,
        vault: &std::path::Path,
        skill: SkillEntry,
    ) -> AppResult<Vec<SkillEntry>> {
        let registry = self
            .skill_registry
            .lock()
            .map_err(|_| AppError::msg("skill_registry_lock_failed"))?;
        let mut skills = registry.get(vault).cloned().unwrap_or_default();
        if let Some(existing) = skills
            .iter_mut()
            .find(|existing| existing.name == skill.name && existing.scope == skill.scope)
        {
            *existing = skill;
        } else {
            skills.push(skill);
        }
        Ok(skills)
    }

    fn prune_pending_tool_calls_locked(
        pending: &mut HashMap<String, PendingToolCall>,
        now: Instant,
    ) {
        let _ = Self::expire_pending_tool_calls_locked(pending, now);
        let overflow = pending.len().saturating_sub(MAX_PENDING_TOOL_CALLS);
        if overflow == 0 {
            return;
        }

        let mut oldest: Vec<(String, Instant)> = pending
            .iter()
            .map(|(id, call)| (id.clone(), call.created_at))
            .collect();
        oldest.sort_by_key(|(_, created_at)| *created_at);
        for (id, _) in oldest.into_iter().take(overflow) {
            pending.remove(&id);
        }
    }

    /// Clear transient in-memory AI state after a vault switch.
    ///
    /// `set_vault` has already installed the new vault's lock-consistent Skill
    /// registry and activation index before this cleanup runs. Clearing that
    /// snapshot here would leave the first normal Run without lexical Skills.
    pub fn clear(&self) {
        if let Ok(mut pending) = self.pending_tool_calls.lock() {
            pending.clear();
        }

        if let Ok(mut classified) = self.classified_ephemeral.lock() {
            classified.clear();
        }
        self.vector_index_ready
            .store(false, std::sync::atomic::Ordering::Relaxed);
        tracing::info!("vault switch: cleared transient AI state and vector readiness");
    }
}

// 鈹€鈹€鈹€ AppState (top-level coordinator) 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

pub struct AppState {
    pub storage: StorageState,
    pub ai: AiRuntimeState,
    pub document_open: DocumentOpenState,
    /// 订阅同步服务：自动/手动刷新共用同一入口与互斥标记。
    pub(crate) feed_sync: FeedSyncService<ProdNetGate>,
    /// RSS 摘要正文的受限后台队列；复用同一 HTTPS/代理安全网门。
    pub(crate) feed_fulltext: FeedFulltextService<ProdNetGate>,
    /// 后台订阅同步设置变化时唤醒 Scheduler；正在执行的批次不被中断。
    pub(crate) feed_sync_wake: Arc<tokio::sync::Notify>,
    vault: Mutex<Option<PathBuf>>,
    paths: IrisPaths,
    pub watcher: Mutex<Option<FileWatcher>>,

    pub db: Arc<Database>,
    pub brute_force: BruteForceProtection,
    /// Test-only local transport injection for deterministic normal-Run tests.
    /// Production builds have neither this field nor its accessor.
    #[cfg(test)]
    test_streaming_client: Mutex<Option<reqwest::Client>>,
}

impl AppState {
    /// Create application state using the production CAS key source.
    pub fn new(data_dir: PathBuf) -> AppResult<Arc<Self>> {
        let home_dir = data_dir
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| data_dir.clone());
        Self::new_with_paths(IrisPaths {
            cache_dir: data_dir.join("cache"),
            temp_dir: data_dir.join("tmp"),
            global_skills_dir: home_dir.join("skills"),
            temp_dir_explicit: false,
            home_dir,
            data_dir,
        })
    }

    /// Create state from the canonical Iris paths resolved at application startup.
    pub fn new_with_paths(paths: IrisPaths) -> AppResult<Arc<Self>> {
        Self::new_with_cas_key_override(paths, None)
    }

    /// Create application state with a deterministic CAS key for integration tests.
    #[doc(hidden)]
    pub fn new_with_test_cas_key(data_dir: PathBuf, cas_key: [u8; 32]) -> AppResult<Arc<Self>> {
        let home_dir = data_dir
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| data_dir.clone());
        Self::new_with_cas_key_override(
            IrisPaths {
                cache_dir: data_dir.join("cache"),
                temp_dir: data_dir.join("tmp"),
                global_skills_dir: home_dir.join("skills"),
                temp_dir_explicit: false,
                home_dir,
                data_dir,
            },
            Some(cas_key),
        )
    }

    fn new_with_cas_key_override(
        paths: IrisPaths,
        cas_key_override: Option<[u8; 32]>,
    ) -> AppResult<Arc<Self>> {
        let db_path = paths.data_dir.join("iris.db");
        let db = Arc::new(Database::open(&db_path)?);
        if let Err(error) = crate::ai_runtime::run_engine::RunEngine::recover_interrupted_runs(&db)
        {
            tracing::warn!("failed to recover interrupted Agent Runs safely: {error}");
        }
        let vector_ready = db.vector_index_ready();

        let storage = StorageState::new(Arc::clone(&db), cas_key_override);
        let ai = AiRuntimeState::new(vector_ready);

        let feed_gate = Arc::new(crate::feed::fetch::ProdNetGate);
        let feed_sync = FeedSyncService::new(Arc::clone(&db), feed_gate.clone());
        let feed_fulltext = FeedFulltextService::new(Arc::clone(&db), feed_gate);
        let state = Arc::new(Self {
            db: Arc::clone(&storage.db),
            storage,
            ai,
            document_open: DocumentOpenState::new(),
            feed_sync,
            feed_fulltext,
            feed_sync_wake: Arc::new(tokio::sync::Notify::new()),
            vault: Mutex::new(None),
            paths,
            watcher: Mutex::new(None),
            brute_force: BruteForceProtection::new(),
            #[cfg(test)]
            test_streaming_client: Mutex::new(None),
        });
        if state.db.with_conn(recover_interrupted_generation).is_err() {
            tracing::warn!(
                result_code = "embedding_recovery_failed",
                "embedding recovery was unavailable"
            );
        }
        let _ = state
            .db
            .with_conn(FeedRepository::recover_interrupted_fulltext);
        if let Err(error) = crate::cache::CacheCoordinator::new(&state.paths, &state.db).maintain()
        {
            tracing::warn!(error_code = %error, "startup cache maintenance failed");
        }

        if let Some(v) = state.load_vault_setting()? {
            let path = PathBuf::from(v);
            if let Err(e) = state.set_vault(path) {
                tracing::warn!("stored vault_path invalid, cleared: {e}");
                state.clear_vault_setting()?;
            }
        }
        state.load_follow_system_proxy_setting();
        Ok(state)
    }

    pub fn is_vector_index_ready(&self) -> bool {
        self.ai
            .vector_index_ready
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Install a local deterministic streaming transport for one test state.
    #[cfg(test)]
    pub(crate) fn set_test_streaming_client(&self, client: reqwest::Client) {
        if let Ok(mut slot) = self.test_streaming_client.lock() {
            *slot = Some(client);
        }
    }

    /// Return the test-only streaming transport without exposing it to production.
    #[cfg(test)]
    pub(crate) fn test_streaming_client(&self) -> Option<reqwest::Client> {
        self.test_streaming_client
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
    }

    pub fn embedding_scheduler(&self) -> Arc<EmbeddingScheduler> {
        Arc::clone(
            self.ai
                .embedding_scheduler
                .get_or_init(|| EmbeddingScheduler::new(Arc::clone(&self.db))),
        )
    }

    pub fn begin_document_open(&self) -> String {
        self.embedding_scheduler().set_foreground_busy(true);
        self.document_open.begin()
    }

    pub fn end_document_open(&self, token: &str) -> bool {
        let ended = self.document_open.end(token);
        if self.document_open.count() == 0 {
            self.embedding_scheduler().set_foreground_busy(false);
        }
        ended
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

    fn load_follow_system_proxy_setting(&self) {
        let follow = self
            .db
            .with_conn(|conn| {
                let result: Result<String, _> = conn.query_row(
                    "SELECT value FROM settings WHERE key = 'follow_system_proxy'",
                    [],
                    |r| r.get(0),
                );
                match result {
                    Ok(json) => {
                        let value: Value = serde_json::from_str(&json)?;
                        Ok(crate::network::parse_follow_system_proxy_setting(Some(
                            &value,
                        )))
                    }
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(true),
                    Err(e) => Err(e.into()),
                }
            })
            .unwrap_or(true);
        crate::network::set_follow_system_proxy(follow);
    }

    pub fn set_vault(&self, path: PathBuf) -> AppResult<()> {
        if !path.is_dir() {
            return Err(AppError::msg("Vault must be a directory"));
        }
        let canonical = path.canonicalize().unwrap_or_else(|_| {
            tracing::warn!(
                result_code = "vault_canonicalize_failed",
                "vault canonicalize failed"
            );
            path
        });
        // This is an explicit vault activation boundary, not a Run. Refresh the
        // loaded Skill registry here so every later Run can stay I/O-free.
        let skills = crate::ai_runtime::skills::scan_all(&canonical)?;
        let embedding_scheduler = self.embedding_scheduler();
        embedding_scheduler.reset_for_vault();
        crate::ai_runtime::skills::rebuild_activation_index(&self.db, &skills)?;
        let activation_index = crate::ai_runtime::skills::load_activation_index(&self.db)?;
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
        })?;
        self.ai.replace_skill_activation_snapshot(
            &embedding_scheduler,
            canonical,
            skills,
            activation_index,
        )?;
        embedding_scheduler.schedule_skill_activation_embeddings();
        Ok(())
    }

    /// Explicitly rescan Skills for a vault. Never call this from a Run path.
    pub fn refresh_skills_for_vault(&self, vault: &std::path::Path) -> AppResult<Vec<SkillEntry>> {
        let skills = crate::ai_runtime::skills::scan_all(vault)?;
        crate::ai_runtime::skills::rebuild_activation_index(&self.db, &skills)?;
        let activation_index = crate::ai_runtime::skills::load_activation_index(&self.db)?;
        let embedding_scheduler = self.embedding_scheduler();
        self.ai.replace_skill_activation_snapshot(
            &embedding_scheduler,
            vault.to_path_buf(),
            skills.clone(),
            activation_index,
        )?;
        embedding_scheduler.schedule_skill_activation_embeddings();
        Ok(skills)
    }

    /// Return the already-loaded Skills for this vault without filesystem I/O.
    pub fn cached_skills_for_vault(
        &self,
        vault: &std::path::Path,
    ) -> AppResult<Option<Vec<SkillEntry>>> {
        self.ai.cached_skills(vault)
    }

    /// Return one lock-consistent Skill registry and activation-index snapshot.
    pub fn cached_skill_activation_for_vault(
        &self,
        vault: &std::path::Path,
    ) -> AppResult<Option<(Vec<SkillEntry>, ActivationIndexMap)>> {
        let embedding_scheduler = self.embedding_scheduler();
        self.ai.cached_skill_activation(&embedding_scheduler, vault)
    }

    /// Update one cached entry after an explicit user-confirmed Skill write.
    pub fn upsert_cached_skill_for_vault(
        &self,
        vault: &std::path::Path,
        skill: SkillEntry,
    ) -> AppResult<()> {
        let skills = self.ai.skills_with_upsert(vault, skill)?;
        crate::ai_runtime::skills::rebuild_activation_index(&self.db, &skills)?;
        let activation_index = crate::ai_runtime::skills::load_activation_index(&self.db)?;
        let embedding_scheduler = self.embedding_scheduler();
        self.ai.replace_skill_activation_snapshot(
            &embedding_scheduler,
            vault.to_path_buf(),
            skills,
            activation_index,
        )?;
        embedding_scheduler.schedule_skill_activation_embeddings();
        Ok(())
    }

    pub fn vault_path(&self) -> AppResult<PathBuf> {
        let guard = self.vault.lock().map_err(|_| AppError::msg("Lock error"))?;
        guard
            .clone()
            .ok_or_else(|| AppError::msg("绗旇鐩綍鏈厤缃紝璇峰厛閫夋嫨 vault"))
    }

    /// Clear transient AI state after `set_vault` installs the new Skill snapshot.
    pub fn clear_ai_state(&self) {
        self.ai.clear();
    }

    pub fn data_dir(&self) -> &PathBuf {
        &self.paths.data_dir
    }

    /// Canonical cache root for all reconstructible disk content.
    pub fn cache_dir(&self) -> &PathBuf {
        &self.paths.cache_dir
    }

    /// Canonical temporary root for transient process artifacts.
    pub fn temp_dir(&self) -> &PathBuf {
        &self.paths.temp_dir
    }

    /// Canonical application paths used by cache governance.
    pub fn paths(&self) -> &IrisPaths {
        &self.paths
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

    struct ReadySkillBatcher;

    impl crate::embedding::scheduler::EmbeddingBatcher for ReadySkillBatcher {
        fn ensure_available(&self) -> AppResult<()> {
            Ok(())
        }

        fn embed_batch(&self, texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
            Ok(vec![
                vec![0.5; crate::embedding::engine::EMBEDDING_DIMENSION];
                texts.len()
            ])
        }
    }

    fn pending_tool_call(id: usize, created_at: Instant) -> PendingToolCall {
        PendingToolCall {
            tool_name: format!("tool-{id}"),
            arguments: "{}".into(),
            request_id: format!("req-{id}"),
            session_id: id as i64,
            note_path: None,
            file_id: None,
            web_search_enabled: false,
            autonomy_level: AutonomyLevel::L1,
            depth: 0,
            skill_activation_plan: None,
            created_at,
        }
    }

    #[test]
    fn embedding_scheduler_does_not_keep_app_state_alive() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::new_with_test_cas_key(dir.path().join("data"), [0xA7; 32]).unwrap();
        let weak = Arc::downgrade(&state);

        state.embedding_scheduler().notify_index_committed();
        drop(state);

        for _ in 0..20 {
            if weak.upgrade().is_none() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert!(
            weak.upgrade().is_none(),
            "embedding queue worker must not keep AppState alive"
        );
    }
    #[test]
    fn pending_tool_calls_expire_returns_removed_entries() {
        let now = Instant::now();
        let mut pending = HashMap::new();
        pending.insert(
            "expired".into(),
            pending_tool_call(1, now - PENDING_TOOL_CALL_TTL - Duration::from_secs(1)),
        );
        pending.insert("fresh".into(), pending_tool_call(2, now));

        let expired = AiRuntimeState::expire_pending_tool_calls_locked(&mut pending, now);

        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].0, "expired");
        assert_eq!(expired[0].1.request_id, "req-1");
        assert!(!pending.contains_key("expired"));
        assert!(pending.contains_key("fresh"));
    }
    #[test]
    fn pending_tool_calls_prune_expired_and_over_capacity_entries() {
        let now = Instant::now();
        let mut pending = HashMap::new();
        pending.insert(
            "expired".into(),
            pending_tool_call(999, now - PENDING_TOOL_CALL_TTL - Duration::from_secs(1)),
        );
        for i in 0..(MAX_PENDING_TOOL_CALLS + 4) {
            pending.insert(
                format!("call-{i}"),
                pending_tool_call(
                    i,
                    now - Duration::from_secs((MAX_PENDING_TOOL_CALLS + 4 - i) as u64),
                ),
            );
        }

        AiRuntimeState::prune_pending_tool_calls_locked(&mut pending, now);

        assert_eq!(pending.len(), MAX_PENDING_TOOL_CALLS);
        assert!(!pending.contains_key("expired"));
        assert!(!pending.contains_key("call-0"));
    }

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

    #[test]
    fn cached_skills_survive_filesystem_changes_until_explicit_refresh() {
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let state =
            AppState::new_with_test_cas_key(directory.path().join("data"), [0xB1; 32]).unwrap();
        state.set_vault(vault.clone()).unwrap();

        let skill_path = vault.join(".iris/skills/cached-skill/SKILL.md");
        let skill_target = std::path::PathBuf::from("cached-skill/SKILL.md");
        let entry = crate::ai_runtime::skills::write_confirmed_skill_content(
            &vault,
            &skill_target,
            crate::ai_runtime::skills::SkillScope::Vault,
            "---\nname: cached-skill\ndescription: Cached run instructions\n---\n\nUse cached instructions.",
        )
        .unwrap();
        state.upsert_cached_skill_for_vault(&vault, entry).unwrap();
        let indexed_description = state
            .db
            .with_read_conn(|conn| {
                conn.query_row(
                    "SELECT description FROM skill_activation_index WHERE skill_name = 'cached-skill' AND scope = 'Vault'",
                    [],
                    |row| row.get::<_, Option<String>>(0),
                )
                .map_err(Into::into)
            })
            .unwrap();
        assert_eq!(
            indexed_description.as_deref(),
            Some("Cached run instructions")
        );
        std::fs::remove_file(&skill_path).unwrap();

        let cached = state
            .cached_skills_for_vault(&vault)
            .unwrap()
            .expect("vault activation owns an in-memory Skill registry");
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].name, "cached-skill");

        let refreshed = state.refresh_skills_for_vault(&vault).unwrap();
        assert!(
            refreshed.is_empty(),
            "only an explicit refresh may observe removal"
        );
    }

    #[test]
    fn confirmed_skill_cache_update_schedules_background_activation_embedding() {
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let state =
            AppState::new_with_test_cas_key(directory.path().join("data"), [0xB2; 32]).unwrap();
        state
            .ai
            .embedding_scheduler
            .set(
                crate::embedding::scheduler::EmbeddingScheduler::with_batcher(
                    Arc::clone(&state.db),
                    Arc::new(ReadySkillBatcher),
                ),
            )
            .ok()
            .expect("install deterministic embedding scheduler");
        state.set_vault(vault.clone()).unwrap();
        let skill = crate::ai_runtime::skills::write_confirmed_skill_content(
            &vault,
            &PathBuf::from("background-skill/SKILL.md"),
            crate::ai_runtime::skills::SkillScope::Vault,
            "---\nname: background-skill\ndescription: Background semantic activation\n---\n\nUse background instructions.",
        )
        .unwrap();

        state.upsert_cached_skill_for_vault(&vault, skill).unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let embedded = state
                .db
                .with_read_conn(|conn| {
                    conn.query_row(
                        "SELECT embedding_model IS NOT NULL
                         FROM skill_activation_index
                         WHERE skill_name = 'background-skill' AND scope = 'Vault'",
                        [],
                        |row| row.get::<_, bool>(0),
                    )
                    .map_err(Into::into)
                })
                .unwrap();
            if embedded {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "cache refresh must trigger background Skill embeddings"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
