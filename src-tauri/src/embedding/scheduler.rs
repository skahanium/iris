use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::engine::{embed_texts_batch, f32_to_bytes, EMBEDDING_DIMENSION, EMBEDDING_MODEL_ID};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

const LEGACY_MODEL_ID: &str = "fastembed/AllMiniLML6V2";
const BATCH_SIZE: usize = 16;
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
    foreground_busy: bool,
    initial_index_complete: bool,
    activity_epoch: u64,
    vault_epoch: u64,
}

/// The single owner for generation work and incremental vector repairs.
pub struct EmbeddingScheduler {
    db: Arc<Database>,
    batcher: Arc<dyn EmbeddingBatcher>,
    runtime: Mutex<RuntimeState>,
    app_handle: Mutex<Option<AppHandle>>,
}

impl EmbeddingScheduler {
    pub fn new(db: Arc<Database>) -> Arc<Self> {
        Self::with_batcher(db, Arc::new(BgeEmbeddingBatcher))
    }

    #[doc(hidden)]
    pub fn with_batcher(db: Arc<Database>, batcher: Arc<dyn EmbeddingBatcher>) -> Arc<Self> {
        Arc::new(Self {
            db,
            batcher,
            runtime: Mutex::new(RuntimeState {
                foreground_busy: true,
                ..RuntimeState::default()
            }),
            app_handle: Mutex::new(None),
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
            runtime.activity_epoch = runtime.activity_epoch.wrapping_add(1);
        }
    }

    pub fn status(&self) -> AppResult<EmbeddingIndexStatus> {
        self.db.with_read_conn(embedding_index_status)
    }

    pub fn enqueue_file(self: &Arc<Self>, _file_id: i64) {
        // A reindex invalidates vectors through FK cascade. Keep one owner for
        // the repair by transitioning a previously-ready generation to paused;
        // the normal idle policy resumes it without a second queue/worker.
        let _ = self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE embedding_generation_state SET phase = 'paused', updated_at = datetime('now') WHERE singleton = 1 AND phase = 'ready'",
                [],
            )?;
            Ok(())
        });
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
        let scheduler = Arc::clone(self);
        thread::Builder::new()
            .name("iris-embedding-scheduler".into())
            .spawn(move || scheduler.run_generation(vault_epoch))
            .map_err(|error| {
                AppError::msg(format!("Failed to start embedding scheduler: {error}"))
            })?;
        Ok(EmbeddingStartResult::Started)
    }

    pub fn mark_initial_index_complete(self: &Arc<Self>) {
        let epoch = match self.runtime.lock() {
            Ok(mut runtime) => {
                runtime.initial_index_complete = true;
                runtime.activity_epoch
            }
            Err(_) => return,
        };
        self.schedule_auto_start(epoch);
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
        let should_resume = {
            let mut runtime = self
                .runtime
                .lock()
                .map_err(|_| AppError::msg("Embedding scheduler lock poisoned"))?;
            runtime.manual_paused = paused;
            !paused
                && !runtime.foreground_busy
                && runtime.initial_index_complete
                && !runtime.running
        };
        if paused {
            self.db.with_conn(set_phase_paused)?;
        } else if should_resume {
            let _ = self.start_generation(EmbeddingStartSource::Manual)?;
        }
        Ok(())
    }

    fn schedule_auto_start(self: &Arc<Self>, epoch: u64) {
        let scheduler = Arc::clone(self);
        let _ = thread::Builder::new()
            .name("iris-embedding-idle".into())
            .spawn(move || {
                thread::sleep(IDLE_DELAY);
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
            return;
        }
        loop {
            if !self.is_current_vault(vault_epoch) {
                self.finish_worker();
                return;
            }
            if self.should_pause() {
                let _ = self.db.with_conn(set_phase_paused);
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
                self.emit_status();
                self.finish_worker();
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
                    self.emit_status();
                    self.finish_worker();
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
                self.emit_status();
                self.finish_worker();
                return;
            }
            self.emit_status();
            thread::yield_now();
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
    fn finish_worker(&self) {
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.running = false;
        }
    }
    fn emit_status(&self) {
        let Ok(status) = self.status() else {
            return;
        };
        if let Ok(handle) = self.app_handle.lock() {
            if let Some(handle) = handle.as_ref() {
                let _ = handle.emit("embedding-index-progress", status);
            }
        }
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

pub fn recover_interrupted_generation(conn: &Connection) -> AppResult<()> {
    conn.execute(
        "UPDATE embedding_generation_state SET phase = 'failed', failure_code = 'interrupted_restart', last_error = ?1, updated_at = datetime('now') WHERE singleton = 1 AND phase IN ('running', 'paused', 'rebuilding')",
        [INTERRUPTED_SUMMARY],
    )?;
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
    use crate::storage::migrate::migrate_up;

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
}
