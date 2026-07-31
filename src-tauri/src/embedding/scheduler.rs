use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::engine::{
    embed_texts_batch, f32_to_bytes, EMBEDDING_DIMENSION, EMBEDDING_MODEL_FINGERPRINT,
    EMBEDDING_MODEL_ID,
};
use crate::ai_runtime::skills::{ActivationIndexMap, SkillActivationIndexRow};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

const LEGACY_MODEL_ID: &str = "fastembed/AllMiniLML6V2";
const BATCH_SIZE: usize = 16;
const MAX_SKILL_QUERY_CACHE_ENTRIES: usize = 64;
const IDLE_DELAY: Duration = Duration::from_secs(30);
const FAILED_SUMMARY: &str = "Embedding rebuild failed";
const INTERRUPTED_SUMMARY: &str = "Embedding rebuild interrupted";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingIndexStatus {
    pub active_model_id: String,
    pub target_model_id: String,
    pub dimension: i64,
    pub phase: String,
    pub indexed_items: i64,
    pub total_items: i64,
    pub last_error: Option<String>,
    pub failure_code: Option<String>,
    pub automatic_attempted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingStartResult {
    Started,
    AlreadyRunning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingStartSource {
    Manual,
    Automatic,
}

/// Model boundary used by the scheduler. Implementations never receive a SQLite connection.
pub trait EmbeddingBatcher: Send + Sync {
    fn ensure_available(&self) -> AppResult<()>;
    fn embed_batch(&self, texts: &[&str]) -> AppResult<Vec<Vec<f32>>>;
}

pub struct BgeEmbeddingBatcher;
impl EmbeddingBatcher for BgeEmbeddingBatcher {
    fn ensure_available(&self) -> AppResult<()> {
        super::engine::ensure_embedding_model_available()
    }
    fn embed_batch(&self, texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
        embed_texts_batch(texts)
    }
}

#[derive(Default)]
struct RuntimeState {
    running: bool,
    manual_paused: bool,
    restart_after_pause: bool,
    foreground_busy: bool,
    initial_index_complete: bool,
    activity_epoch: u64,
    vault_epoch: u64,
    skill_activation_epoch: Option<u64>,
    skill_activation_reschedule: bool,
}

/// The single owner for generation work and incremental vector repairs.
pub struct EmbeddingScheduler {
    db: Arc<Database>,
    batcher: Arc<dyn EmbeddingBatcher>,
    idle_delay: Duration,
    runtime: Mutex<RuntimeState>,
    skill_query_embeddings: Mutex<HashMap<String, Vec<f32>>>,
    skill_activation_index: Mutex<ActivationIndexMap>,
    app_handle: Mutex<Option<AppHandle>>,
    #[cfg(test)]
    emitted_statuses: Mutex<Vec<EmbeddingIndexStatus>>,
}

impl EmbeddingScheduler {
    pub fn new(db: Arc<Database>) -> Arc<Self> {
        Self::with_batcher(db, Arc::new(BgeEmbeddingBatcher))
    }

    #[doc(hidden)]
    pub fn with_batcher(db: Arc<Database>, batcher: Arc<dyn EmbeddingBatcher>) -> Arc<Self> {
        Self::with_batcher_and_idle_delay(db, batcher, IDLE_DELAY)
    }

    /// Construct a scheduler with a deterministic idle delay for contract tests.
    #[doc(hidden)]
    pub fn with_batcher_and_idle_delay(
        db: Arc<Database>,
        batcher: Arc<dyn EmbeddingBatcher>,
        idle_delay: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            db,
            batcher,
            idle_delay,
            runtime: Mutex::new(RuntimeState {
                foreground_busy: true,
                ..RuntimeState::default()
            }),
            skill_query_embeddings: Mutex::new(HashMap::new()),
            skill_activation_index: Mutex::new(ActivationIndexMap::new()),
            app_handle: Mutex::new(None),
            #[cfg(test)]
            emitted_statuses: Mutex::new(Vec::new()),
        })
    }

    pub fn attach_app_handle(&self, app_handle: AppHandle) {
        if let Ok(mut handle) = self.app_handle.lock() {
            *handle = Some(app_handle);
        }
    }

    /// Invalidate a worker snapshot when the active vault changes.
    pub fn reset_for_vault(&self) {
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.vault_epoch = runtime.vault_epoch.wrapping_add(1);
            runtime.initial_index_complete = false;
            runtime.foreground_busy = true;
            runtime.restart_after_pause = false;
            runtime.activity_epoch = runtime.activity_epoch.wrapping_add(1);
            runtime.skill_activation_epoch = None;
            runtime.skill_activation_reschedule = false;
        }
        if let Ok(mut queries) = self.skill_query_embeddings.lock() {
            queries.clear();
        }
        if let Ok(mut index) = self.skill_activation_index.lock() {
            index.clear();
        }
    }

    /// Atomically replace the active vault's in-memory Skill activation index.
    pub fn replace_skill_activation_index(&self, mut index: ActivationIndexMap) -> AppResult<()> {
        let _runtime = self
            .runtime
            .lock()
            .map_err(|_| AppError::msg("Embedding scheduler lock poisoned"))?;
        let committed = crate::ai_runtime::skills::load_activation_index(&self.db)?;
        let mut cached = self
            .skill_activation_index
            .lock()
            .map_err(|_| AppError::msg("Skill activation index cache lock poisoned"))?;
        for (key, incoming) in &mut index {
            if current_skill_activation_vector(incoming) {
                continue;
            }
            let matching = committed
                .get(key)
                .filter(|row| {
                    row.embedding_source_hash == incoming.embedding_source_hash
                        && current_skill_activation_vector(row)
                })
                .or_else(|| {
                    cached.get(key).filter(|row| {
                        row.embedding_source_hash == incoming.embedding_source_hash
                            && current_skill_activation_vector(row)
                    })
                });
            if let Some(existing) = matching {
                incoming.embedding_json = existing.embedding_json.clone();
                incoming.embedding_model = existing.embedding_model.clone();
                incoming.embedding_dimensions = existing.embedding_dimensions;
            }
        }
        *cached = index;
        Ok(())
    }

    /// Read the active vault's Skill activation index without SQLite access.
    pub fn cached_skill_activation_index(&self) -> ActivationIndexMap {
        self.skill_activation_index
            .lock()
            .map(|index| index.clone())
            .unwrap_or_default()
    }

    /// Start a vault-scoped background repair for missing Skill activation vectors.
    ///
    /// Lexical index replacement and the confirmed in-memory registry are
    /// committed by the caller before this method is invoked. The worker never
    /// blocks a Run and an old-vault result cannot cross `reset_for_vault`.
    pub fn schedule_skill_activation_embeddings(self: &Arc<Self>) {
        let epoch = match self.runtime.lock() {
            Ok(mut runtime) => {
                let epoch = runtime.vault_epoch;
                if runtime.skill_activation_epoch == Some(epoch) {
                    runtime.skill_activation_reschedule = true;
                    return;
                }
                runtime.skill_activation_epoch = Some(epoch);
                runtime.skill_activation_reschedule = false;
                epoch
            }
            Err(_) => return,
        };
        let scheduler = Arc::clone(self);
        if thread::Builder::new()
            .name("iris-skill-activation-embeddings".into())
            .spawn(move || scheduler.run_skill_activation_generation(epoch))
            .is_err()
        {
            let _ = self.finish_skill_activation_worker(epoch);
        }
    }

    /// Precompute one composer query outside the Run execution path.
    ///
    /// Production calls this from a best-effort debounced UI request. A cache
    /// miss or model failure therefore changes only ranking quality: Runs use
    /// the deterministic lexical order without loading or waiting for a model.
    pub fn prepare_skill_activation_query(&self, query: &str) -> AppResult<()> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(());
        }
        let vault_epoch = self
            .runtime
            .lock()
            .map_err(|_| AppError::msg("Embedding scheduler lock poisoned"))?
            .vault_epoch;
        if self.cached_skill_activation_query(query).is_some() {
            return Ok(());
        }
        self.batcher.ensure_available()?;
        let mut embeddings = self.batcher.embed_batch(&[query])?;
        if embeddings.len() != 1 {
            return Err(AppError::Embed(
                "Skill activation query embedding count mismatch".into(),
            ));
        }
        let embedding = embeddings.remove(0);
        if embedding.len() != EMBEDDING_DIMENSION {
            return Err(AppError::Embed(format!(
                "Skill activation query returned {} dimensions, expected {EMBEDDING_DIMENSION}",
                embedding.len()
            )));
        }
        let runtime = self
            .runtime
            .lock()
            .map_err(|_| AppError::msg("Embedding scheduler lock poisoned"))?;
        if runtime.vault_epoch != vault_epoch {
            return Ok(());
        }
        let mut cache = self
            .skill_query_embeddings
            .lock()
            .map_err(|_| AppError::msg("Skill query embedding cache lock poisoned"))?;
        if cache.len() >= MAX_SKILL_QUERY_CACHE_ENTRIES {
            cache.clear();
        }
        cache.insert(query.to_string(), embedding);
        Ok(())
    }

    /// Read a prepared query vector without filesystem or model access.
    pub fn cached_skill_activation_query(&self, query: &str) -> Option<Vec<f32>> {
        self.skill_query_embeddings
            .lock()
            .ok()
            .and_then(|cache| cache.get(query.trim()).cloned())
    }

    #[cfg(test)]
    pub(crate) fn cache_skill_activation_query_for_test(&self, query: &str, embedding: Vec<f32>) {
        self.skill_query_embeddings
            .lock()
            .expect("Skill query embedding cache")
            .insert(query.trim().to_string(), embedding);
    }

    pub fn status(&self) -> AppResult<EmbeddingIndexStatus> {
        self.db.with_read_conn(embedding_index_status)
    }

    /// Record that a derived Markdown index transaction has committed.
    ///
    /// Callers must invoke this only after their `Database::with_conn` scope has
    /// ended. A reindex invalidates vectors through FK cascade; the scheduler
    /// remains the sole owner of the repair state and resumes through its normal
    /// idle policy without a second queue or worker.
    pub fn notify_index_committed(self: &Arc<Self>) {
        let transitioned = self.db.with_conn(|conn| {
            if generation_coverage_complete(conn)? {
                return Ok(false);
            }
            Ok(conn.execute(
                "UPDATE embedding_generation_state SET phase = 'paused', updated_at = datetime('now') WHERE singleton = 1 AND phase = 'ready'",
                [],
            )? > 0)
        });
        let Ok(true) = transitioned else {
            return;
        };
        self.emit_status();
        let epoch = self.runtime.lock().ok().and_then(|runtime| {
            (!runtime.foreground_busy
                && runtime.initial_index_complete
                && !runtime.manual_paused
                && !runtime.running)
                .then_some(runtime.activity_epoch)
        });
        if let Some(epoch) = epoch {
            self.schedule_auto_start(epoch);
        }
    }

    pub fn start_generation(
        self: &Arc<Self>,
        source: EmbeddingStartSource,
    ) -> AppResult<EmbeddingStartResult> {
        {
            let mut runtime = self
                .runtime
                .lock()
                .map_err(|_| AppError::msg("Embedding scheduler lock poisoned"))?;
            if runtime.running {
                return Ok(EmbeddingStartResult::AlreadyRunning);
            }
            if source == EmbeddingStartSource::Automatic
                && (!runtime.initial_index_complete
                    || runtime.foreground_busy
                    || runtime.manual_paused)
            {
                return Ok(EmbeddingStartResult::AlreadyRunning);
            }
            runtime.running = true;
        }
        let (transition, vault_epoch) = {
            let runtime = self
                .runtime
                .lock()
                .map_err(|_| AppError::msg("Embedding scheduler lock poisoned"))?;
            (
                self.db.with_conn(|conn| transition_running(conn, source)),
                runtime.vault_epoch,
            )
        };
        if let Err(error) = transition {
            if let Ok(mut runtime) = self.runtime.lock() {
                runtime.running = false;
            }
            return Err(error);
        }
        if !transition? {
            if let Ok(mut runtime) = self.runtime.lock() {
                runtime.running = false;
            }
            return Ok(EmbeddingStartResult::AlreadyRunning);
        }
        self.emit_status();
        let scheduler = Arc::clone(self);
        if let Err(error) = thread::Builder::new()
            .name("iris-embedding-scheduler".into())
            .spawn(move || scheduler.run_generation(vault_epoch))
        {
            self.handle_worker_spawn_failure(vault_epoch);
            return Err(AppError::msg(format!(
                "Failed to start embedding scheduler: {error}"
            )));
        }
        Ok(EmbeddingStartResult::Started)
    }

    pub fn mark_initial_index_complete(self: &Arc<Self>) {
        let epoch = match self.runtime.lock() {
            Ok(mut runtime) => {
                runtime.initial_index_complete = true;
                (!runtime.foreground_busy && !runtime.manual_paused && !runtime.running)
                    .then_some(runtime.activity_epoch)
            }
            Err(_) => return,
        };
        if let Some(epoch) = epoch {
            self.schedule_auto_start(epoch);
        }
    }

    pub fn set_foreground_busy(self: &Arc<Self>, busy: bool) {
        let epoch = match self.runtime.lock() {
            Ok(mut runtime) => {
                runtime.foreground_busy = busy;
                runtime.activity_epoch = runtime.activity_epoch.wrapping_add(1);
                runtime.activity_epoch
            }
            Err(_) => return,
        };
        if !busy {
            self.schedule_auto_start(epoch);
        }
    }

    pub fn set_manual_paused(self: &Arc<Self>, paused: bool) -> AppResult<()> {
        let should_start_now = {
            let mut runtime = self
                .runtime
                .lock()
                .map_err(|_| AppError::msg("Embedding scheduler lock poisoned"))?;
            runtime.manual_paused = paused;
            if paused {
                runtime.restart_after_pause = false;
                return Ok(());
            }
            !runtime.foreground_busy && runtime.initial_index_complete && !runtime.running
        };
        if should_start_now {
            let _ = self.start_generation(EmbeddingStartSource::Manual)?;
        } else if self.status()?.phase == "paused" {
            if let Ok(mut runtime) = self.runtime.lock() {
                if runtime.running && !runtime.manual_paused && !runtime.foreground_busy {
                    runtime.restart_after_pause = true;
                }
            }
        }
        Ok(())
    }

    fn schedule_auto_start(self: &Arc<Self>, epoch: u64) {
        let scheduler = Arc::clone(self);
        let _ = thread::Builder::new()
            .name("iris-embedding-idle".into())
            .spawn(move || {
                thread::sleep(scheduler.idle_delay);
                let allowed = scheduler.runtime.lock().is_ok_and(|runtime| {
                    runtime.activity_epoch == epoch
                        && runtime.initial_index_complete
                        && !runtime.foreground_busy
                        && !runtime.manual_paused
                        && !runtime.running
                });
                if allowed {
                    let _ = scheduler.start_generation(EmbeddingStartSource::Automatic);
                }
            });
    }

    fn run_generation(self: Arc<Self>, vault_epoch: u64) {
        let result = self.batcher.ensure_available();
        if result.is_err() {
            let _ = self.write_if_current(vault_epoch, |conn| {
                mark_failed(conn, "model_unavailable", "Embedding model unavailable")
            });
            self.finish_worker();
            self.emit_status();
            return;
        }
        loop {
            if !self.is_current_vault(vault_epoch) {
                self.finish_worker();
                return;
            }
            if self.should_pause() && self.pause_if_current(vault_epoch).unwrap_or(false) {
                self.emit_status();
                self.finish_worker();
                return;
            }
            let batch = match self.db.with_read_conn(load_pending_batch) {
                Ok(batch) => batch,
                Err(_) => {
                    let _ = self.write_if_current(vault_epoch, |conn| {
                        mark_failed(conn, "database_error", "Embedding database unavailable")
                    });
                    self.finish_worker();
                    self.emit_status();
                    return;
                }
            };
            if batch.is_empty() {
                let completion = self.write_if_current(vault_epoch, finalize_if_covered);
                if matches!(completion, Ok(false)) {
                    self.finish_worker();
                    return;
                }
                if completion.is_err() {
                    let _ = self.write_if_current(vault_epoch, |conn| {
                        mark_failed(conn, "database_error", "Embedding database unavailable")
                    });
                }
                self.finish_worker();
                self.emit_status();
                return;
            }
            let texts = batch
                .iter()
                .map(|record| record.text.as_str())
                .collect::<Vec<_>>();
            let vectors = match self.batcher.embed_batch(&texts) {
                Ok(vectors)
                    if vectors.len() == batch.len()
                        && vectors
                            .iter()
                            .all(|vector| vector.len() == EMBEDDING_DIMENSION) =>
                {
                    vectors
                }
                _ => {
                    let _ = self.write_if_current(vault_epoch, |conn| {
                        mark_failed(conn, "embedding_failed", FAILED_SUMMARY)
                    });
                    self.finish_worker();
                    self.emit_status();
                    return;
                }
            };
            let committed =
                self.write_if_current(vault_epoch, |conn| commit_batch(conn, &batch, &vectors));
            if matches!(committed, Ok(false)) {
                self.finish_worker();
                return;
            }
            if committed.is_err() {
                let _ = self.write_if_current(vault_epoch, |conn| {
                    mark_failed(conn, "database_error", "Embedding database unavailable")
                });
                self.finish_worker();
                self.emit_status();
                return;
            }
            self.emit_status();
            thread::yield_now();
        }
    }

    fn run_skill_activation_generation(self: Arc<Self>, vault_epoch: u64) {
        let mut model_ready = false;
        loop {
            if !self.is_current_vault(vault_epoch) {
                break;
            }
            let batch = match self.db.with_read_conn(load_pending_skill_activation_batch) {
                Ok(batch) => batch,
                Err(_) => break,
            };
            if batch.is_empty() {
                break;
            }
            if !model_ready {
                if self.batcher.ensure_available().is_err() {
                    break;
                }
                model_ready = true;
            }
            let texts = batch
                .iter()
                .map(|record| record.text.as_str())
                .collect::<Vec<_>>();
            let vectors = match self.batcher.embed_batch(&texts) {
                Ok(vectors)
                    if vectors.len() == batch.len()
                        && vectors
                            .iter()
                            .all(|vector| vector.len() == EMBEDDING_DIMENSION) =>
                {
                    vectors
                }
                _ => break,
            };
            let mut applied = Vec::new();
            if !matches!(
                self.write_if_current(vault_epoch, |conn| {
                    applied = commit_skill_activation_batch(conn, &batch, &vectors)?;
                    Ok(())
                }),
                Ok(true)
            ) {
                break;
            }
            self.cache_skill_activation_batch(vault_epoch, &batch, &vectors, &applied);
            thread::yield_now();
        }
        if self.finish_skill_activation_worker(vault_epoch) {
            self.schedule_skill_activation_embeddings();
        }
    }

    fn should_pause(&self) -> bool {
        self.runtime
            .lock()
            .map(|runtime| runtime.manual_paused || runtime.foreground_busy)
            .unwrap_or(true)
    }
    fn is_current_vault(&self, vault_epoch: u64) -> bool {
        self.runtime
            .lock()
            .map(|runtime| runtime.vault_epoch == vault_epoch)
            .unwrap_or(false)
    }
    /// Hold the epoch gate across a short write transaction. A vault reset
    /// either happens before this gate (the write is skipped) or after the
    /// transaction commits; an old inference result can never cross the reset.
    fn write_if_current<T>(
        &self,
        vault_epoch: u64,
        write: impl FnOnce(&Connection) -> AppResult<T>,
    ) -> AppResult<bool> {
        let runtime = self
            .runtime
            .lock()
            .map_err(|_| AppError::msg("Embedding scheduler lock poisoned"))?;
        if runtime.vault_epoch != vault_epoch {
            return Ok(false);
        }
        self.db.with_conn(write)?;
        Ok(true)
    }
    fn pause_if_current(&self, vault_epoch: u64) -> AppResult<bool> {
        let runtime = self
            .runtime
            .lock()
            .map_err(|_| AppError::msg("Embedding scheduler lock poisoned"))?;
        if runtime.vault_epoch != vault_epoch
            || (!runtime.manual_paused && !runtime.foreground_busy)
        {
            return Ok(false);
        }
        self.db.with_conn(set_phase_paused)?;
        Ok(true)
    }
    fn finish_worker(self: &Arc<Self>) {
        let restart = if let Ok(mut runtime) = self.runtime.lock() {
            runtime.running = false;
            if runtime.restart_after_pause
                && !runtime.manual_paused
                && !runtime.foreground_busy
                && runtime.initial_index_complete
            {
                runtime.restart_after_pause = false;
                true
            } else {
                false
            }
        } else {
            false
        };
        if restart {
            let _ = self.start_generation(EmbeddingStartSource::Manual);
        }
    }

    fn finish_skill_activation_worker(&self, vault_epoch: u64) -> bool {
        if let Ok(mut runtime) = self.runtime.lock() {
            if runtime.skill_activation_epoch == Some(vault_epoch) {
                runtime.skill_activation_epoch = None;
                let restart = runtime.skill_activation_reschedule;
                runtime.skill_activation_reschedule = false;
                return restart;
            }
        }
        false
    }

    fn cache_skill_activation_batch(
        &self,
        vault_epoch: u64,
        batch: &[PendingSkillActivationRecord],
        vectors: &[Vec<f32>],
        applied: &[bool],
    ) {
        let Ok(runtime) = self.runtime.lock() else {
            return;
        };
        if runtime.vault_epoch != vault_epoch {
            return;
        }
        let Ok(mut index) = self.skill_activation_index.lock() else {
            return;
        };
        for ((record, vector), applied) in batch.iter().zip(vectors).zip(applied) {
            if !applied {
                continue;
            }
            let scope = if record.scope == "Vault" {
                crate::ai_runtime::skills::SkillScope::Vault
            } else {
                crate::ai_runtime::skills::SkillScope::Global
            };
            let Some(row) = index.get_mut(&(record.skill_name.clone(), scope)) else {
                continue;
            };
            if row.embedding_source_hash != record.source_hash {
                continue;
            }
            let Ok(embedding_json) = serde_json::to_string(vector) else {
                continue;
            };
            row.embedding_json = Some(embedding_json);
            row.embedding_model = Some(EMBEDDING_MODEL_FINGERPRINT.into());
            row.embedding_dimensions = Some(EMBEDDING_DIMENSION as i64);
        }
    }

    fn handle_worker_spawn_failure(self: &Arc<Self>, vault_epoch: u64) {
        let _ = self.write_if_current(vault_epoch, |conn| {
            mark_failed(
                conn,
                "scheduler_start_failed",
                "Embedding scheduler unavailable",
            )
        });
        self.finish_worker();
        self.emit_status();
    }

    fn emit_status(&self) {
        let Ok(status) = self.status() else {
            return;
        };
        #[cfg(test)]
        if let Ok(mut emitted) = self.emitted_statuses.lock() {
            emitted.push(status.clone());
        }
        if let Ok(handle) = self.app_handle.lock() {
            if let Some(handle) = handle.as_ref() {
                let _ = handle.emit("embedding-index-progress", status);
            }
        }
    }

    #[cfg(test)]
    fn emitted_statuses(&self) -> Vec<EmbeddingIndexStatus> {
        self.emitted_statuses
            .lock()
            .map(|statuses| statuses.clone())
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy)]
enum RecordKind {
    Chunk,
    Anchor,
    Regulation,
}
struct PendingRecord {
    kind: RecordKind,
    id: i64,
    text: String,
    fingerprint: String,
}

struct PendingSkillActivationRecord {
    skill_name: String,
    scope: String,
    text: String,
    source_hash: String,
}

fn current_skill_activation_vector(row: &SkillActivationIndexRow) -> bool {
    let expected_source_hash = crate::ai_runtime::skills::activation_embedding_source_hash(
        &row.skill_name,
        row.description.as_deref().unwrap_or_default(),
        row.keywords.as_deref().unwrap_or_default(),
    );
    if row.embedding_source_hash != expected_source_hash
        || row.embedding_model.as_deref() != Some(EMBEDDING_MODEL_FINGERPRINT)
        || row.embedding_dimensions != Some(EMBEDDING_DIMENSION as i64)
    {
        return false;
    }
    row.embedding_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<Vec<f32>>(json).ok())
        .is_some_and(|vector| {
            vector.len() == EMBEDDING_DIMENSION && vector.iter().all(|value| value.is_finite())
        })
}

fn load_pending_skill_activation_batch(
    conn: &Connection,
) -> AppResult<Vec<PendingSkillActivationRecord>> {
    let mut statement = conn.prepare(
        "SELECT skill_name, scope, COALESCE(description, ''),
                COALESCE(keywords, ''), embedding_source_hash
         FROM skill_activation_index
         WHERE embedding_source_hash <> ''
           AND (
               embedding_json IS NULL
               OR embedding_model IS NULL
               OR embedding_model <> ?1
               OR embedding_dimensions IS NULL
               OR embedding_dimensions <> ?2
               OR CASE
                    WHEN json_valid(embedding_json) = 1
                    THEN json_array_length(embedding_json) <> ?2
                    ELSE 1
                  END
           )
         ORDER BY scope, skill_name
         LIMIT ?3",
    )?;
    let rows = statement.query_map(
        rusqlite::params![
            EMBEDDING_MODEL_FINGERPRINT,
            EMBEDDING_DIMENSION as i64,
            BATCH_SIZE as i64
        ],
        |row| {
            let skill_name = row.get::<_, String>(0)?;
            let scope = row.get::<_, String>(1)?;
            let description = row.get::<_, String>(2)?;
            let keywords = row.get::<_, String>(3)?;
            Ok(PendingSkillActivationRecord {
                text: crate::ai_runtime::skills::activation_embedding_source(
                    &skill_name,
                    &description,
                    &keywords,
                ),
                skill_name,
                scope,
                source_hash: row.get(4)?,
            })
        },
    )?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn commit_skill_activation_batch(
    conn: &Connection,
    batch: &[PendingSkillActivationRecord],
    vectors: &[Vec<f32>],
) -> AppResult<Vec<bool>> {
    let transaction = conn.unchecked_transaction()?;
    let mut applied = Vec::with_capacity(batch.len());
    for (record, vector) in batch.iter().zip(vectors) {
        let embedding_json = serde_json::to_string(vector)?;
        let updated = transaction.execute(
            "UPDATE skill_activation_index
             SET embedding_json = ?1,
                 embedding_model = ?2,
                 embedding_dimensions = ?3,
                 updated_at = datetime('now')
             WHERE skill_name = ?4
               AND scope = ?5
               AND embedding_source_hash = ?6",
            rusqlite::params![
                embedding_json,
                EMBEDDING_MODEL_FINGERPRINT,
                EMBEDDING_DIMENSION as i64,
                record.skill_name,
                record.scope,
                record.source_hash,
            ],
        )?;
        applied.push(updated == 1);
    }
    transaction.commit()?;
    Ok(applied)
}

pub fn recover_interrupted_generation(conn: &Connection) -> AppResult<()> {
    let phase = conn
        .query_row(
            "SELECT phase FROM embedding_generation_state WHERE singleton = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(phase) = phase else {
        return Ok(());
    };
    if !matches!(phase.as_str(), "running" | "paused" | "rebuilding") {
        return Ok(());
    }

    let total = total_sources(conn)?;
    let indexed = valid_sources(conn)?;
    if indexed == total {
        conn.execute(
            "UPDATE embedding_generation_state
             SET active_model_id = ?1, target_model_id = ?1, target_dimension = ?2,
                 phase = 'ready', indexed_items = ?3, total_items = ?3,
                 last_error = NULL, failure_code = NULL, updated_at = datetime('now')
             WHERE singleton = 1 AND phase IN ('running', 'paused', 'rebuilding')",
            rusqlite::params![EMBEDDING_MODEL_ID, EMBEDDING_DIMENSION as i64, total],
        )?;
    } else {
        conn.execute(
            "UPDATE embedding_generation_state
             SET target_model_id = ?1, target_dimension = ?2,
                 phase = 'failed', indexed_items = ?3, total_items = ?4,
                 failure_code = 'interrupted_restart', last_error = ?5,
                 updated_at = datetime('now')
             WHERE singleton = 1 AND phase IN ('running', 'paused', 'rebuilding')",
            rusqlite::params![
                EMBEDDING_MODEL_ID,
                EMBEDDING_DIMENSION as i64,
                indexed,
                total,
                INTERRUPTED_SUMMARY,
            ],
        )?;
    }
    Ok(())
}

pub fn embedding_index_status(conn: &Connection) -> AppResult<EmbeddingIndexStatus> {
    let status = conn.query_row(
        "SELECT active_model_id, target_model_id, target_dimension, phase, indexed_items, total_items, last_error, failure_code, automatic_attempted FROM embedding_generation_state WHERE singleton = 1",
        [], |row| Ok(EmbeddingIndexStatus { active_model_id: row.get(0)?, target_model_id: row.get(1)?, dimension: row.get(2)?, phase: row.get(3)?, indexed_items: row.get(4)?, total_items: row.get(5)?, last_error: row.get(6)?, failure_code: row.get(7)?, automatic_attempted: row.get::<_, i64>(8)? != 0 }),
    ).optional();
    match status {
        Ok(Some(status)) => Ok(status),
        Ok(None) => Ok(legacy_ready_status()),
        Err(error) if unavailable_schema(&error) => Ok(legacy_ready_status()),
        Err(error) => Err(error.into()),
    }
}

pub fn generation_coverage_complete(conn: &Connection) -> AppResult<bool> {
    let expected = total_sources(conn)?;
    let actual = valid_sources(conn)?;
    Ok(expected == actual)
}

fn transition_running(conn: &Connection, source: EmbeddingStartSource) -> AppResult<bool> {
    let status = embedding_index_status(conn)?;
    if status.phase == "running" {
        return Ok(false);
    }
    if source == EmbeddingStartSource::Automatic
        && !((status.phase == "legacy_ready" && !status.automatic_attempted)
            || status.phase == "paused")
    {
        return Ok(false);
    }
    if !matches!(
        status.phase.as_str(),
        "legacy_ready" | "failed" | "ready" | "paused"
    ) {
        return Ok(false);
    }
    conn.execute(
        "UPDATE embedding_generation_state SET target_model_id = ?1, target_dimension = ?2, phase = 'running', last_error = NULL, failure_code = NULL, automatic_attempted = CASE WHEN ?3 THEN 1 ELSE automatic_attempted END, updated_at = datetime('now') WHERE singleton = 1",
        rusqlite::params![EMBEDDING_MODEL_ID, EMBEDDING_DIMENSION as i64, source == EmbeddingStartSource::Automatic],
    )?;
    Ok(true)
}

fn set_phase_paused(conn: &Connection) -> AppResult<()> {
    conn.execute("UPDATE embedding_generation_state SET phase = 'paused', updated_at = datetime('now') WHERE singleton = 1 AND phase = 'running'", [])?;
    Ok(())
}

fn mark_failed(conn: &Connection, code: &str, summary: &str) -> AppResult<()> {
    let total = total_sources(conn)?;
    let indexed = valid_sources(conn)?;
    conn.execute(
        "UPDATE embedding_generation_state SET active_model_id = CASE WHEN ?1 = 'model_unavailable' THEN active_model_id ELSE active_model_id END, target_model_id = ?2, target_dimension = ?3, phase = 'failed', indexed_items = ?4, total_items = ?5, failure_code = ?1, last_error = ?6, updated_at = datetime('now') WHERE singleton = 1",
        rusqlite::params![code, EMBEDDING_MODEL_ID, EMBEDDING_DIMENSION as i64, indexed, total, summary],
    )?;
    Ok(())
}

fn load_pending_batch(conn: &Connection) -> AppResult<Vec<PendingRecord>> {
    let mut records = missing_records(conn, RecordKind::Chunk, BATCH_SIZE)?;
    if records.len() < BATCH_SIZE {
        records.extend(missing_records(
            conn,
            RecordKind::Anchor,
            BATCH_SIZE - records.len(),
        )?);
    }
    if records.len() < BATCH_SIZE {
        records.extend(missing_records(
            conn,
            RecordKind::Regulation,
            BATCH_SIZE - records.len(),
        )?);
    }
    Ok(records)
}

fn missing_records(
    conn: &Connection,
    kind: RecordKind,
    limit: usize,
) -> AppResult<Vec<PendingRecord>> {
    let (sql, query): (&str, &str) = match kind {
        RecordKind::Chunk => ("SELECT c.id, COALESCE(f.title, '') || char(10) || COALESCE(c.heading_path, '') || char(10) || COALESCE(m.aliases, '') || char(10) || COALESCE(m.tags, '') || char(10) || c.content, COALESCE(c.content_hash, '') FROM chunks c JOIN files f ON f.id = c.file_id LEFT JOIN files_metadata_fts m ON m.path = f.path LEFT JOIN chunk_embeddings_v2 e ON e.chunk_id = c.id WHERE e.chunk_id IS NULL OR e.model_id <> ?1 OR e.dimension <> ?2 OR e.source_fingerprint <> COALESCE(c.content_hash, '') OR length(e.embedding) <> ?3 ORDER BY c.id LIMIT ?4", "chunks"),
        RecordKind::Anchor => ("SELECT a.id, a.content, COALESCE(a.content_hash, '') FROM semantic_anchors a LEFT JOIN semantic_anchor_embeddings_v2 e ON e.anchor_id = a.id WHERE e.anchor_id IS NULL OR e.model_id <> ?1 OR e.dimension <> ?2 OR e.source_fingerprint <> COALESCE(a.content_hash, '') OR length(e.embedding) <> ?3 ORDER BY a.id LIMIT ?4", "anchors"),
        RecordKind::Regulation => ("SELECT r.id, r.content, COALESCE(r.content_hash, '') FROM regulation_index r LEFT JOIN regulation_embeddings_v2 e ON e.regulation_id = r.id WHERE e.regulation_id IS NULL OR e.model_id <> ?1 OR e.dimension <> ?2 OR e.source_fingerprint <> COALESCE(r.content_hash, '') OR length(e.embedding) <> ?3 ORDER BY r.id LIMIT ?4", "regulations"),
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(
        rusqlite::params![
            EMBEDDING_MODEL_ID,
            EMBEDDING_DIMENSION as i64,
            (EMBEDDING_DIMENSION * std::mem::size_of::<f32>()) as i64,
            limit as i64
        ],
        |row| {
            Ok(PendingRecord {
                kind,
                id: row.get(0)?,
                text: row.get(1)?,
                fingerprint: row.get(2)?,
            })
        },
    )?;
    let _ = query;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn commit_batch(conn: &Connection, batch: &[PendingRecord], vectors: &[Vec<f32>]) -> AppResult<()> {
    let transaction = conn.unchecked_transaction()?;
    for (record, vector) in batch.iter().zip(vectors) {
        let (source_sql, table, column) = match record.kind {
            RecordKind::Chunk => (
                "SELECT COALESCE(content_hash, '') FROM chunks WHERE id = ?1",
                "chunk_embeddings_v2",
                "chunk_id",
            ),
            RecordKind::Anchor => (
                "SELECT COALESCE(content_hash, '') FROM semantic_anchors WHERE id = ?1",
                "semantic_anchor_embeddings_v2",
                "anchor_id",
            ),
            RecordKind::Regulation => (
                "SELECT COALESCE(content_hash, '') FROM regulation_index WHERE id = ?1",
                "regulation_embeddings_v2",
                "regulation_id",
            ),
        };
        let current: Option<String> = transaction
            .query_row(source_sql, [record.id], |row| row.get(0))
            .optional()?;
        if current.as_deref() != Some(record.fingerprint.as_str()) {
            continue;
        }
        transaction.execute(&format!("INSERT INTO {table} ({column}, embedding, source_fingerprint, model_id, dimension) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT({column}) DO UPDATE SET embedding = excluded.embedding, source_fingerprint = excluded.source_fingerprint, model_id = excluded.model_id, dimension = excluded.dimension"), rusqlite::params![record.id, f32_to_bytes(vector), record.fingerprint, EMBEDDING_MODEL_ID, EMBEDDING_DIMENSION as i64])?;
    }
    refresh_progress(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn finalize_if_covered(conn: &Connection) -> AppResult<()> {
    refresh_progress(conn)?;
    if generation_coverage_complete(conn)? {
        let total = total_sources(conn)?;
        conn.execute("UPDATE embedding_generation_state SET active_model_id = ?1, target_model_id = ?1, target_dimension = ?2, phase = 'ready', indexed_items = ?3, total_items = ?3, last_error = NULL, failure_code = NULL, updated_at = datetime('now') WHERE singleton = 1 AND phase = 'running'", rusqlite::params![EMBEDDING_MODEL_ID, EMBEDDING_DIMENSION as i64, total])?;
    }
    Ok(())
}

fn refresh_progress(conn: &Connection) -> AppResult<()> {
    let total = total_sources(conn)?;
    let indexed = valid_sources(conn)?;
    conn.execute("UPDATE embedding_generation_state SET indexed_items = ?1, total_items = ?2, updated_at = datetime('now') WHERE singleton = 1", rusqlite::params![indexed, total])?;
    Ok(())
}

fn total_sources(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row("SELECT (SELECT COUNT(*) FROM chunks) + (SELECT COUNT(*) FROM semantic_anchors) + (SELECT COUNT(*) FROM regulation_index)", [], |row| row.get(0))?)
}
fn valid_sources(conn: &Connection) -> AppResult<i64> {
    let bytes = (EMBEDDING_DIMENSION * std::mem::size_of::<f32>()) as i64;
    let sql = "SELECT (SELECT COUNT(*) FROM chunks c JOIN chunk_embeddings_v2 e ON e.chunk_id = c.id WHERE e.model_id = ?1 AND e.dimension = ?2 AND e.source_fingerprint = COALESCE(c.content_hash, '') AND length(e.embedding) = ?3) + (SELECT COUNT(*) FROM semantic_anchors a JOIN semantic_anchor_embeddings_v2 e ON e.anchor_id = a.id WHERE e.model_id = ?1 AND e.dimension = ?2 AND e.source_fingerprint = COALESCE(a.content_hash, '') AND length(e.embedding) = ?3) + (SELECT COUNT(*) FROM regulation_index r JOIN regulation_embeddings_v2 e ON e.regulation_id = r.id WHERE e.model_id = ?1 AND e.dimension = ?2 AND e.source_fingerprint = COALESCE(r.content_hash, '') AND length(e.embedding) = ?3)";
    Ok(conn.query_row(
        sql,
        rusqlite::params![EMBEDDING_MODEL_ID, EMBEDDING_DIMENSION as i64, bytes],
        |row| row.get(0),
    )?)
}

fn legacy_ready_status() -> EmbeddingIndexStatus {
    EmbeddingIndexStatus {
        active_model_id: LEGACY_MODEL_ID.into(),
        target_model_id: EMBEDDING_MODEL_ID.into(),
        dimension: EMBEDDING_DIMENSION as i64,
        phase: "legacy_ready".into(),
        indexed_items: 0,
        total_items: 0,
        last_error: None,
        failure_code: None,
        automatic_attempted: false,
    }
}
fn unavailable_schema(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(_, Some(detail)) if detail.contains("no such table"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::Database;
    use crate::storage::migrate::migrate_up;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    struct ReadyBatcher;

    impl EmbeddingBatcher for ReadyBatcher {
        fn ensure_available(&self) -> AppResult<()> {
            Ok(())
        }

        fn embed_batch(&self, texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
            Ok(vec![vec![0.5; EMBEDDING_DIMENSION]; texts.len()])
        }
    }

    struct UnavailableBatcher;

    impl EmbeddingBatcher for UnavailableBatcher {
        fn ensure_available(&self) -> AppResult<()> {
            Err(AppError::Embed("unavailable model".into()))
        }

        fn embed_batch(&self, _texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
            unreachable!("model preflight prevents batches")
        }
    }

    struct CountingBatcher {
        ensure_calls: AtomicUsize,
        batch_calls: AtomicUsize,
    }

    impl EmbeddingBatcher for CountingBatcher {
        fn ensure_available(&self) -> AppResult<()> {
            self.ensure_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn embed_batch(&self, texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
            self.batch_calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![vec![0.5; EMBEDDING_DIMENSION]; texts.len()])
        }
    }

    struct BlockingQueryBatcher {
        started: Mutex<Option<mpsc::Sender<()>>>,
        release: Mutex<mpsc::Receiver<()>>,
    }

    impl EmbeddingBatcher for BlockingQueryBatcher {
        fn ensure_available(&self) -> AppResult<()> {
            Ok(())
        }

        fn embed_batch(&self, texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
            if let Some(started) = self.started.lock().unwrap().take() {
                started.send(()).unwrap();
            }
            self.release.lock().unwrap().recv().unwrap();
            Ok(vec![vec![0.5; EMBEDDING_DIMENSION]; texts.len()])
        }
    }

    struct BlockingThenFailBatcher {
        started: Mutex<Option<mpsc::Sender<()>>>,
        release: Mutex<mpsc::Receiver<()>>,
        calls: AtomicUsize,
    }

    impl EmbeddingBatcher for BlockingThenFailBatcher {
        fn ensure_available(&self) -> AppResult<()> {
            Ok(())
        }

        fn embed_batch(&self, texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
            if self.calls.fetch_add(1, Ordering::SeqCst) > 0 {
                return Err(AppError::Embed("stop after changed-source window".into()));
            }
            if let Some(started) = self.started.lock().unwrap().take() {
                started.send(()).unwrap();
            }
            self.release.lock().unwrap().recv().unwrap();
            Ok(vec![vec![0.5; EMBEDDING_DIMENSION]; texts.len()])
        }
    }

    fn seed_chunk(conn: &Connection) {
        conn.execute(
            "INSERT INTO files(path,title,content_hash,word_count,created_at,updated_at)
             VALUES ('note.md','Note','file',1,'now','now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chunks(file_id,chunk_index,content,content_hash)
             VALUES (1,0,'body','chunk')",
            [],
        )
        .unwrap();
    }

    fn seed_covered_chunks(conn: &Connection, count: usize) -> Vec<i64> {
        conn.execute(
            "INSERT INTO files(path,title,content_hash,word_count,created_at,updated_at)
             VALUES ('note.md','Note','file',1,'now','now')",
            [],
        )
        .unwrap();
        let file_id = conn.last_insert_rowid();
        (0..count)
            .map(|index| {
                let fingerprint = format!("chunk-{index}");
                conn.execute(
                    "INSERT INTO chunks(file_id,chunk_index,content,content_hash)
                     VALUES (?1,?2,'body',?3)",
                    rusqlite::params![file_id, index as i64, fingerprint],
                )
                .unwrap();
                conn.last_insert_rowid()
            })
            .collect()
    }

    fn seed_valid_vector(conn: &Connection, chunk_id: i64, fingerprint: &str) {
        conn.execute(
            "INSERT INTO chunk_embeddings_v2(chunk_id,embedding,source_fingerprint,model_id,dimension)
             VALUES (?1,?2,?3,?4,?5)",
            rusqlite::params![
                chunk_id,
                f32_to_bytes(&vec![0.5; EMBEDDING_DIMENSION]),
                fingerprint,
                EMBEDDING_MODEL_ID,
                EMBEDDING_DIMENSION as i64,
            ],
        )
        .unwrap();
    }

    fn set_generation_phase(conn: &Connection, phase: &str) {
        conn.execute(
            "UPDATE embedding_generation_state
             SET active_model_id = ?1, target_model_id = ?1, target_dimension = ?2,
                 phase = ?3, indexed_items = 0, total_items = 0,
                 last_error = 'stale', failure_code = 'stale'
             WHERE singleton = 1",
            rusqlite::params![EMBEDDING_MODEL_ID, EMBEDDING_DIMENSION as i64, phase],
        )
        .unwrap();
    }

    fn wait_for_phase(scheduler: &EmbeddingScheduler, expected: &str) {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while !scheduler
            .emitted_statuses()
            .iter()
            .any(|status| status.phase == expected)
        {
            assert!(
                std::time::Instant::now() < deadline,
                "scheduler did not emit phase {expected}"
            );
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(scheduler.status().unwrap().phase, expected);
    }

    #[test]
    fn unknown_vector_metadata_does_not_count_as_coverage() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();
        conn.execute("INSERT INTO files(path,title,content_hash,word_count,created_at,updated_at) VALUES ('a.md','A','f',1,'now','now')", []).unwrap();
        conn.execute("INSERT INTO chunks(file_id,chunk_index,content,content_hash) VALUES (1,0,'body','fingerprint')", []).unwrap();
        conn.execute(
            "INSERT INTO chunk_embeddings_v2(chunk_id,embedding) VALUES (1, zeroblob(2048))",
            [],
        )
        .unwrap();
        assert!(!generation_coverage_complete(&conn).unwrap());
    }

    #[test]
    fn emits_complete_snapshots_for_running_paused_and_ready_transitions() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        db.with_conn(|conn| {
            seed_chunk(conn);
            Ok(())
        })
        .unwrap();
        let scheduler = EmbeddingScheduler::with_batcher(Arc::clone(&db), Arc::new(ReadyBatcher));
        scheduler.set_foreground_busy(false);
        scheduler.set_manual_paused(true).unwrap();
        scheduler.mark_initial_index_complete();
        scheduler
            .start_generation(EmbeddingStartSource::Manual)
            .unwrap();
        wait_for_phase(&scheduler, "paused");
        scheduler.set_manual_paused(false).unwrap();
        wait_for_phase(&scheduler, "ready");

        let phases = scheduler
            .emitted_statuses()
            .into_iter()
            .map(|status| status.phase)
            .collect::<Vec<_>>();
        assert!(phases.contains(&"running".to_string()));
        assert!(phases.contains(&"paused".to_string()));
        assert!(phases.contains(&"ready".to_string()));
    }

    #[test]
    fn emits_safe_failed_snapshot_when_model_preflight_is_unavailable() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        db.with_conn(|conn| {
            seed_chunk(conn);
            Ok(())
        })
        .unwrap();
        let scheduler =
            EmbeddingScheduler::with_batcher(Arc::clone(&db), Arc::new(UnavailableBatcher));
        scheduler.set_foreground_busy(false);
        scheduler
            .start_generation(EmbeddingStartSource::Manual)
            .unwrap();
        wait_for_phase(&scheduler, "failed");

        let failed = scheduler
            .emitted_statuses()
            .into_iter()
            .find(|status| status.phase == "failed")
            .expect("failed state is emitted");
        assert_eq!(failed.failure_code.as_deref(), Some("model_unavailable"));
        assert_eq!(
            failed.last_error.as_deref(),
            Some("Embedding model unavailable")
        );
    }

    #[test]
    fn enqueue_repair_emits_paused_snapshot_before_idle_restart() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        db.with_conn(|conn| {
            seed_chunk(conn);
            conn.execute(
                "UPDATE embedding_generation_state SET phase = 'ready' WHERE singleton = 1",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        let scheduler =
            EmbeddingScheduler::with_batcher(Arc::clone(&db), Arc::new(UnavailableBatcher));

        scheduler.notify_index_committed();

        assert_eq!(scheduler.status().unwrap().phase, "paused");
        assert_eq!(
            scheduler
                .emitted_statuses()
                .last()
                .map(|status| status.phase.as_str()),
            Some("paused")
        );
    }

    #[test]
    fn unchanged_index_notification_keeps_complete_generation_ready_without_loading_model() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        db.with_conn(|conn| {
            let chunk_id = seed_covered_chunks(conn, 1)[0];
            seed_valid_vector(conn, chunk_id, "chunk-0");
            set_generation_phase(conn, "ready");
            refresh_progress(conn)
        })
        .unwrap();
        let batcher = Arc::new(CountingBatcher {
            ensure_calls: AtomicUsize::new(0),
            batch_calls: AtomicUsize::new(0),
        });
        let scheduler = EmbeddingScheduler::with_batcher(Arc::clone(&db), batcher.clone());

        scheduler.notify_index_committed();

        assert_eq!(scheduler.status().unwrap().phase, "ready");
        assert!(scheduler.emitted_statuses().is_empty());
        assert_eq!(batcher.ensure_calls.load(Ordering::SeqCst), 0);
        assert_eq!(batcher.batch_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn complete_interrupted_generation_recovers_to_ready_for_running_and_paused_phases() {
        for phase in ["running", "paused"] {
            let conn = Connection::open_in_memory().unwrap();
            migrate_up(&conn).unwrap();
            let chunk_id = seed_covered_chunks(&conn, 1)[0];
            seed_valid_vector(&conn, chunk_id, "chunk-0");
            set_generation_phase(&conn, phase);

            recover_interrupted_generation(&conn).unwrap();

            let status = embedding_index_status(&conn).unwrap();
            assert_eq!(status.phase, "ready", "phase {phase}");
            assert_eq!((status.indexed_items, status.total_items), (1, 1));
            assert_eq!(status.last_error, None);
            assert_eq!(status.failure_code, None);
            assert!(super::super::engine::embedding_generation_ready(&conn).unwrap());
        }
    }

    #[test]
    fn reopening_complete_generation_preserves_semantic_readiness_without_new_model_work() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("iris.db");
        let db = Arc::new(Database::open(&db_path).unwrap());
        db.with_conn(|conn| {
            let chunk_id = seed_covered_chunks(conn, 1)[0];
            seed_valid_vector(conn, chunk_id, "chunk-0");
            set_generation_phase(conn, "running");
            Ok(())
        })
        .unwrap();
        drop(db);

        let reopened = Arc::new(Database::open(&db_path).unwrap());
        reopened.with_conn(recover_interrupted_generation).unwrap();
        let batcher = Arc::new(CountingBatcher {
            ensure_calls: AtomicUsize::new(0),
            batch_calls: AtomicUsize::new(0),
        });
        let scheduler = EmbeddingScheduler::with_batcher_and_idle_delay(
            Arc::clone(&reopened),
            batcher.clone(),
            Duration::from_millis(5),
        );

        scheduler.set_foreground_busy(false);
        scheduler.notify_index_committed();
        scheduler.mark_initial_index_complete();
        thread::sleep(Duration::from_millis(25));

        reopened
            .with_read_conn(|conn| {
                assert!(super::super::engine::embedding_generation_ready(conn)?);
                Ok(())
            })
            .unwrap();
        assert_eq!(scheduler.status().unwrap().phase, "ready");
        assert_eq!(batcher.ensure_calls.load(Ordering::SeqCst), 0);
        assert_eq!(batcher.batch_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn incomplete_interrupted_generation_fails_without_deleting_valid_batches() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();
        let chunk_ids = seed_covered_chunks(&conn, 2);
        seed_valid_vector(&conn, chunk_ids[0], "chunk-0");
        set_generation_phase(&conn, "paused");

        recover_interrupted_generation(&conn).unwrap();

        let status = embedding_index_status(&conn).unwrap();
        assert_eq!(status.phase, "failed");
        assert_eq!(status.failure_code.as_deref(), Some("interrupted_restart"));
        assert_eq!((status.indexed_items, status.total_items), (1, 2));
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM chunk_embeddings_v2 WHERE chunk_id = ?1",
                [chunk_ids[0]],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            1
        );
        assert!(!generation_coverage_complete(&conn).unwrap());
    }

    #[test]
    fn interrupted_recovery_rejects_mismatched_vector_metadata() {
        for (label, mutation) in [
            (
                "model",
                "UPDATE chunk_embeddings_v2 SET model_id = 'other-model'",
            ),
            (
                "dimension",
                "UPDATE chunk_embeddings_v2 SET dimension = 384",
            ),
            (
                "fingerprint",
                "UPDATE chunk_embeddings_v2 SET source_fingerprint = 'stale'",
            ),
            (
                "vector length",
                "UPDATE chunk_embeddings_v2 SET embedding = zeroblob(4)",
            ),
        ] {
            let conn = Connection::open_in_memory().unwrap();
            migrate_up(&conn).unwrap();
            let chunk_id = seed_covered_chunks(&conn, 1)[0];
            seed_valid_vector(&conn, chunk_id, "chunk-0");
            conn.execute(mutation, []).unwrap();
            set_generation_phase(&conn, "running");

            recover_interrupted_generation(&conn).unwrap();

            let status = embedding_index_status(&conn).unwrap();
            assert_eq!(status.phase, "failed", "mismatched {label}");
            assert_eq!(
                status.failure_code.as_deref(),
                Some("interrupted_restart"),
                "mismatched {label}"
            );
        }
    }

    #[test]
    fn worker_spawn_failure_releases_running_flag_and_emits_safe_failure() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        db.with_conn(|conn| {
            seed_chunk(conn);
            transition_running(conn, EmbeddingStartSource::Manual)?;
            Ok(())
        })
        .unwrap();
        let scheduler =
            EmbeddingScheduler::with_batcher(Arc::clone(&db), Arc::new(UnavailableBatcher));
        scheduler.runtime.lock().unwrap().running = true;

        scheduler.handle_worker_spawn_failure(0);

        assert!(!scheduler.runtime.lock().unwrap().running);
        let failed = scheduler
            .emitted_statuses()
            .into_iter()
            .find(|status| status.phase == "failed")
            .expect("spawn failure emits a failed snapshot");
        assert_eq!(
            failed.failure_code.as_deref(),
            Some("scheduler_start_failed")
        );
        assert_eq!(
            failed.last_error.as_deref(),
            Some("Embedding scheduler unavailable")
        );
    }

    #[test]
    fn skill_activation_worker_generates_missing_vectors_with_source_metadata() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let skill = crate::ai_runtime::skills::SkillEntry {
            name: "semantic-skill".into(),
            description: "Summarize launch readiness".into(),
            scope: crate::ai_runtime::skills::SkillScope::Vault,
            enabled: true,
            confirmation_status: crate::ai_runtime::skills::SkillConfirmationStatus::Confirmed,
            ..Default::default()
        };
        crate::ai_runtime::skills::rebuild_activation_index(&db, std::slice::from_ref(&skill))
            .unwrap();
        let scheduler = EmbeddingScheduler::with_batcher(Arc::clone(&db), Arc::new(ReadyBatcher));
        scheduler
            .replace_skill_activation_index(
                crate::ai_runtime::skills::load_activation_index(&db).unwrap(),
            )
            .unwrap();

        scheduler.schedule_skill_activation_embeddings();

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let stored = db
                .with_read_conn(|conn| {
                    conn.query_row(
                        "SELECT embedding_json, embedding_source_hash,
                                embedding_model, embedding_dimensions
                         FROM skill_activation_index
                         WHERE skill_name = 'semantic-skill' AND scope = 'Vault'",
                        [],
                        |row| {
                            Ok((
                                row.get::<_, Option<String>>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, Option<String>>(2)?,
                                row.get::<_, Option<i64>>(3)?,
                            ))
                        },
                    )
                    .map_err(Into::into)
                })
                .unwrap();
            if stored.0.is_some() {
                assert!(!stored.1.is_empty());
                assert_eq!(stored.2.as_deref(), Some(EMBEDDING_MODEL_FINGERPRINT));
                assert_eq!(stored.3, Some(EMBEDDING_DIMENSION as i64));
                let cached = scheduler.cached_skill_activation_index();
                let cached_row = cached
                    .get(&(
                        "semantic-skill".to_string(),
                        crate::ai_runtime::skills::SkillScope::Vault,
                    ))
                    .expect("cached Skill activation row");
                assert!(cached_row.embedding_json.is_some());
                assert_eq!(
                    cached_row.embedding_model.as_deref(),
                    Some(EMBEDDING_MODEL_FINGERPRINT)
                );
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "skill activation embedding worker did not finish"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn activation_index_replacement_preserves_matching_background_vector() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let skill = crate::ai_runtime::skills::SkillEntry {
            name: "concurrent-vector".into(),
            description: "A stable semantic source".into(),
            scope: crate::ai_runtime::skills::SkillScope::Vault,
            enabled: true,
            confirmation_status: crate::ai_runtime::skills::SkillConfirmationStatus::Confirmed,
            ..Default::default()
        };
        crate::ai_runtime::skills::rebuild_activation_index(&db, std::slice::from_ref(&skill))
            .unwrap();
        let scheduler = EmbeddingScheduler::with_batcher(Arc::clone(&db), Arc::new(ReadyBatcher));
        let missing = crate::ai_runtime::skills::load_activation_index(&db).unwrap();
        scheduler
            .replace_skill_activation_index(missing.clone())
            .unwrap();
        let row = missing
            .get(&(
                "concurrent-vector".to_string(),
                crate::ai_runtime::skills::SkillScope::Vault,
            ))
            .unwrap();
        let batch = vec![PendingSkillActivationRecord {
            skill_name: "concurrent-vector".into(),
            scope: "Vault".into(),
            text: "unused".into(),
            source_hash: row.embedding_source_hash.clone(),
        }];
        scheduler.cache_skill_activation_batch(
            0,
            &batch,
            &[vec![0.5; EMBEDDING_DIMENSION]],
            &[true],
        );

        scheduler.replace_skill_activation_index(missing).unwrap();

        let cached = scheduler.cached_skill_activation_index();
        let row = cached
            .get(&(
                "concurrent-vector".to_string(),
                crate::ai_runtime::skills::SkillScope::Vault,
            ))
            .unwrap();
        assert!(
            row.embedding_json.is_some(),
            "a refresh loaded before the worker commit must not erase its matching vector"
        );
    }

    #[test]
    fn refresh_snapshot_recovers_committed_new_source_vector_while_cache_is_old() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let mut skill = crate::ai_runtime::skills::SkillEntry {
            name: "refresh-source-race".into(),
            description: "Old activation source".into(),
            scope: crate::ai_runtime::skills::SkillScope::Vault,
            enabled: true,
            confirmation_status: crate::ai_runtime::skills::SkillConfirmationStatus::Confirmed,
            ..Default::default()
        };
        crate::ai_runtime::skills::rebuild_activation_index(&db, std::slice::from_ref(&skill))
            .unwrap();
        let old_snapshot = crate::ai_runtime::skills::load_activation_index(&db).unwrap();
        let old_source_hash = old_snapshot
            .get(&(
                "refresh-source-race".to_string(),
                crate::ai_runtime::skills::SkillScope::Vault,
            ))
            .expect("old activation row")
            .embedding_source_hash
            .clone();
        let scheduler = EmbeddingScheduler::with_batcher(Arc::clone(&db), Arc::new(ReadyBatcher));
        scheduler
            .replace_skill_activation_index(old_snapshot)
            .unwrap();

        skill.description = "New activation source".into();
        crate::ai_runtime::skills::rebuild_activation_index(&db, &[skill]).unwrap();
        let refresh_snapshot = crate::ai_runtime::skills::load_activation_index(&db).unwrap();
        let refresh_row = refresh_snapshot
            .get(&(
                "refresh-source-race".to_string(),
                crate::ai_runtime::skills::SkillScope::Vault,
            ))
            .expect("new activation row");
        assert_ne!(refresh_row.embedding_source_hash, old_source_hash);
        assert!(refresh_row.embedding_json.is_none());
        let new_source_hash = refresh_row.embedding_source_hash.clone();
        let batch = [PendingSkillActivationRecord {
            skill_name: "refresh-source-race".into(),
            scope: "Vault".into(),
            text: crate::ai_runtime::skills::activation_embedding_source(
                &refresh_row.skill_name,
                refresh_row.description.as_deref().unwrap_or_default(),
                refresh_row.keywords.as_deref().unwrap_or_default(),
            ),
            source_hash: new_source_hash.clone(),
        }];
        let vectors = [vec![0.5; EMBEDDING_DIMENSION]];
        let applied = db
            .with_conn(|conn| commit_skill_activation_batch(conn, &batch, &vectors))
            .unwrap();
        assert_eq!(
            applied,
            vec![true],
            "the new-source vector must be committed before snapshot replacement"
        );
        assert!(
            db.with_read_conn(load_pending_skill_activation_batch)
                .unwrap()
                .is_empty(),
            "the committed DB vector leaves no repair work"
        );
        let committed = crate::ai_runtime::skills::load_activation_index(&db).unwrap();
        assert!(
            current_skill_activation_vector(
                committed
                    .get(&(
                        "refresh-source-race".to_string(),
                        crate::ai_runtime::skills::SkillScope::Vault,
                    ))
                    .expect("committed new-source row")
            ),
            "DB must contain a valid new-source vector before replace"
        );
        let cached_before_replace = scheduler.cached_skill_activation_index();
        let cached_old_row = cached_before_replace
            .get(&(
                "refresh-source-race".to_string(),
                crate::ai_runtime::skills::SkillScope::Vault,
            ))
            .expect("old cached activation row");
        assert_eq!(cached_old_row.embedding_source_hash, old_source_hash);
        assert!(
            !current_skill_activation_vector(cached_old_row),
            "cache must still expose the old source before replace"
        );

        scheduler
            .replace_skill_activation_index(refresh_snapshot)
            .unwrap();

        let cached = scheduler.cached_skill_activation_index();
        let row = cached
            .get(&(
                "refresh-source-race".to_string(),
                crate::ai_runtime::skills::SkillScope::Vault,
            ))
            .expect("refreshed activation row");
        assert_eq!(row.embedding_source_hash, new_source_hash);
        assert!(
            current_skill_activation_vector(row),
            "refresh must recover the valid new-source vector already committed in DB"
        );
    }

    #[test]
    fn activation_index_replacement_prefers_worker_vector_over_malformed_loaded_snapshot() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let skill = crate::ai_runtime::skills::SkillEntry {
            name: "repair-race".into(),
            description: "A stable semantic source".into(),
            scope: crate::ai_runtime::skills::SkillScope::Vault,
            enabled: true,
            confirmation_status: crate::ai_runtime::skills::SkillConfirmationStatus::Confirmed,
            ..Default::default()
        };
        crate::ai_runtime::skills::rebuild_activation_index(&db, std::slice::from_ref(&skill))
            .unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE skill_activation_index
                 SET embedding_json = 'not-json',
                     embedding_model = ?1,
                     embedding_dimensions = ?2
                 WHERE skill_name = 'repair-race' AND scope = 'Vault'",
                rusqlite::params![EMBEDDING_MODEL_FINGERPRINT, EMBEDDING_DIMENSION as i64],
            )?;
            Ok(())
        })
        .unwrap();
        let malformed_snapshot = crate::ai_runtime::skills::load_activation_index(&db).unwrap();
        let source_hash = malformed_snapshot
            .get(&(
                "repair-race".to_string(),
                crate::ai_runtime::skills::SkillScope::Vault,
            ))
            .expect("malformed activation row")
            .embedding_source_hash
            .clone();
        let scheduler = EmbeddingScheduler::with_batcher(Arc::clone(&db), Arc::new(ReadyBatcher));
        scheduler
            .replace_skill_activation_index(malformed_snapshot.clone())
            .unwrap();
        scheduler.cache_skill_activation_batch(
            0,
            &[PendingSkillActivationRecord {
                skill_name: "repair-race".into(),
                scope: "Vault".into(),
                text: "unused".into(),
                source_hash,
            }],
            &[vec![0.5; EMBEDDING_DIMENSION]],
            &[true],
        );

        scheduler
            .replace_skill_activation_index(malformed_snapshot)
            .unwrap();

        let cached = scheduler.cached_skill_activation_index();
        let row = cached
            .get(&(
                "repair-race".to_string(),
                crate::ai_runtime::skills::SkillScope::Vault,
            ))
            .expect("cached activation row");
        let vector = serde_json::from_str::<Vec<f32>>(
            row.embedding_json
                .as_deref()
                .expect("worker vector must survive replacement"),
        )
        .expect("cached worker vector must remain valid JSON");
        assert_eq!(vector.len(), EMBEDDING_DIMENSION);
    }

    #[test]
    fn activation_index_replacement_rejects_vector_with_forged_source_hash() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let skill = crate::ai_runtime::skills::SkillEntry {
            name: "source-identity".into(),
            description: "The canonical activation source".into(),
            scope: crate::ai_runtime::skills::SkillScope::Vault,
            enabled: true,
            confirmation_status: crate::ai_runtime::skills::SkillConfirmationStatus::Confirmed,
            ..Default::default()
        };
        crate::ai_runtime::skills::rebuild_activation_index(&db, &[skill]).unwrap();
        let mut missing = crate::ai_runtime::skills::load_activation_index(&db).unwrap();
        let row = missing
            .get_mut(&(
                "source-identity".to_string(),
                crate::ai_runtime::skills::SkillScope::Vault,
            ))
            .expect("activation row");
        row.embedding_source_hash = "forged-source-hash".into();
        let mut forged_vector = missing.clone();
        let row = forged_vector
            .get_mut(&(
                "source-identity".to_string(),
                crate::ai_runtime::skills::SkillScope::Vault,
            ))
            .expect("forged vector row");
        row.embedding_json = Some(serde_json::to_string(&vec![0.5; EMBEDDING_DIMENSION]).unwrap());
        row.embedding_model = Some(EMBEDDING_MODEL_FINGERPRINT.into());
        row.embedding_dimensions = Some(EMBEDDING_DIMENSION as i64);
        let scheduler = EmbeddingScheduler::with_batcher(Arc::clone(&db), Arc::new(ReadyBatcher));
        scheduler
            .replace_skill_activation_index(forged_vector)
            .unwrap();

        scheduler.replace_skill_activation_index(missing).unwrap();

        let cached = scheduler.cached_skill_activation_index();
        assert!(
            cached
                .get(&(
                    "source-identity".to_string(),
                    crate::ai_runtime::skills::SkillScope::Vault,
                ))
                .expect("cached activation row")
                .embedding_json
                .is_none(),
            "a source hash that does not identify the indexed embedding source must degrade to lexical"
        );
    }

    #[test]
    fn changed_source_rejects_old_worker_result_from_memory_cache() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let mut skill = crate::ai_runtime::skills::SkillEntry {
            name: "changed-source-race".into(),
            description: "Old activation source".into(),
            scope: crate::ai_runtime::skills::SkillScope::Vault,
            enabled: true,
            confirmation_status: crate::ai_runtime::skills::SkillConfirmationStatus::Confirmed,
            ..Default::default()
        };
        crate::ai_runtime::skills::rebuild_activation_index(&db, std::slice::from_ref(&skill))
            .unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let scheduler = EmbeddingScheduler::with_batcher(
            Arc::clone(&db),
            Arc::new(BlockingThenFailBatcher {
                started: Mutex::new(Some(started_tx)),
                release: Mutex::new(release_rx),
                calls: AtomicUsize::new(0),
            }),
        );
        scheduler
            .replace_skill_activation_index(
                crate::ai_runtime::skills::load_activation_index(&db).unwrap(),
            )
            .unwrap();
        scheduler.schedule_skill_activation_embeddings();
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("old-source worker must reach inference");

        skill.description = "Changed activation source".into();
        crate::ai_runtime::skills::rebuild_activation_index(&db, &[skill]).unwrap();
        release_tx.send(()).unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while scheduler
            .runtime
            .lock()
            .unwrap()
            .skill_activation_epoch
            .is_some()
        {
            assert!(
                std::time::Instant::now() < deadline,
                "changed-source worker did not finish"
            );
            thread::sleep(Duration::from_millis(5));
        }
        let cached = scheduler.cached_skill_activation_index();
        let row = cached
            .get(&(
                "changed-source-race".to_string(),
                crate::ai_runtime::skills::SkillScope::Vault,
            ))
            .expect("old lexical snapshot remains until refresh replacement");
        assert!(
            row.embedding_json.is_none(),
            "a DB-rejected old-source result must not be published to memory"
        );
    }

    #[test]
    fn prepared_skill_query_embedding_is_cached_without_run_time_model_work() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let batcher = Arc::new(CountingBatcher {
            ensure_calls: AtomicUsize::new(0),
            batch_calls: AtomicUsize::new(0),
        });
        let scheduler = EmbeddingScheduler::with_batcher(Arc::clone(&db), batcher.clone());

        assert!(scheduler
            .cached_skill_activation_query("发布准备情况")
            .is_none());
        scheduler
            .prepare_skill_activation_query("发布准备情况")
            .unwrap();

        assert_eq!(
            scheduler
                .cached_skill_activation_query(" 发布准备情况 ")
                .expect("prepared query vector")
                .len(),
            EMBEDDING_DIMENSION
        );
        assert_eq!(batcher.ensure_calls.load(Ordering::SeqCst), 1);
        assert_eq!(batcher.batch_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn vault_reset_discards_in_flight_skill_query_embedding() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let scheduler = EmbeddingScheduler::with_batcher(
            Arc::clone(&db),
            Arc::new(BlockingQueryBatcher {
                started: Mutex::new(Some(started_tx)),
                release: Mutex::new(release_rx),
            }),
        );
        let worker = {
            let scheduler = Arc::clone(&scheduler);
            thread::spawn(move || {
                scheduler
                    .prepare_skill_activation_query("old vault query")
                    .unwrap();
            })
        };
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("query embedding started");

        scheduler.reset_for_vault();
        release_tx.send(()).unwrap();
        worker.join().unwrap();

        assert!(
            scheduler
                .cached_skill_activation_query("old vault query")
                .is_none(),
            "an old-vault query must not be inserted after reset clears the cache"
        );
    }

    #[test]
    fn skill_activation_worker_failure_leaves_lexical_index_usable() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let skill = crate::ai_runtime::skills::SkillEntry {
            name: "knowledge-fallback".into(),
            description: "Answer questions from notes".into(),
            scope: crate::ai_runtime::skills::SkillScope::Vault,
            enabled: true,
            legacy_trigger: Some("knowledge".into()),
            confirmation_status: crate::ai_runtime::skills::SkillConfirmationStatus::Confirmed,
            ..Default::default()
        };
        crate::ai_runtime::skills::rebuild_activation_index(&db, std::slice::from_ref(&skill))
            .unwrap();
        let scheduler =
            EmbeddingScheduler::with_batcher(Arc::clone(&db), Arc::new(UnavailableBatcher));

        scheduler.schedule_skill_activation_embeddings();

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while scheduler
            .runtime
            .lock()
            .unwrap()
            .skill_activation_epoch
            .is_some()
        {
            assert!(
                std::time::Instant::now() < deadline,
                "failed Skill activation worker did not release its epoch"
            );
            thread::sleep(Duration::from_millis(5));
        }
        let embedding = db
            .with_read_conn(|conn| {
                conn.query_row(
                    "SELECT embedding_json
                     FROM skill_activation_index
                     WHERE skill_name = 'knowledge-fallback' AND scope = 'Vault'",
                    [],
                    |row| row.get::<_, Option<String>>(0),
                )
                .map_err(Into::into)
            })
            .unwrap();
        assert!(embedding.is_none());
        let index = crate::ai_runtime::skills::load_activation_index(&db).unwrap();

        let plan = crate::ai_runtime::skills::build_skill_activation_plan_for_task(
            std::slice::from_ref(&skill),
            crate::ai_runtime::AgentIntent::AskNotes,
            "",
            &[],
            Some(&index),
        );

        assert_eq!(plan.activated_skills[0].name, "knowledge-fallback");
    }

    #[test]
    fn skill_activation_worker_repairs_malformed_vector_json() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let skill = crate::ai_runtime::skills::SkillEntry {
            name: "malformed-vector".into(),
            description: "Repair invalid cached embeddings".into(),
            scope: crate::ai_runtime::skills::SkillScope::Vault,
            enabled: true,
            confirmation_status: crate::ai_runtime::skills::SkillConfirmationStatus::Confirmed,
            ..Default::default()
        };
        crate::ai_runtime::skills::rebuild_activation_index(&db, std::slice::from_ref(&skill))
            .unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE skill_activation_index
                 SET embedding_json = 'not-json',
                     embedding_model = ?1,
                     embedding_dimensions = ?2
                 WHERE skill_name = 'malformed-vector' AND scope = 'Vault'",
                rusqlite::params![EMBEDDING_MODEL_FINGERPRINT, EMBEDDING_DIMENSION as i64],
            )?;
            Ok(())
        })
        .unwrap();
        let scheduler = EmbeddingScheduler::with_batcher(Arc::clone(&db), Arc::new(ReadyBatcher));

        scheduler.schedule_skill_activation_embeddings();

        // 全量测试并行执行时，后台 worker 线程的调度可能被显著延迟；
        // 单跑约 0.1s 的修复在 5s 窗口下偶发超时，故放宽为 30s 再断言。
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            let embedding = db
                .with_read_conn(|conn| {
                    conn.query_row(
                        "SELECT embedding_json
                         FROM skill_activation_index
                         WHERE skill_name = 'malformed-vector' AND scope = 'Vault'",
                        [],
                        |row| row.get::<_, Option<String>>(0),
                    )
                    .map_err(Into::into)
                })
                .unwrap();
            if embedding.as_deref() != Some("not-json") {
                let vector = serde_json::from_str::<Vec<f32>>(
                    embedding.as_deref().expect("repaired embedding"),
                )
                .expect("valid repaired embedding");
                assert_eq!(vector.len(), EMBEDDING_DIMENSION);
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "malformed Skill activation vector was not repaired"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
}
