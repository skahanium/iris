use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use super::engine::{embed_texts_batch, f32_to_bytes, EMBEDDING_DIMENSION, EMBEDDING_MODEL_ID};
use crate::error::{AppError, AppResult};

const LEGACY_MODEL_ID: &str = "fastembed/AllMiniLML6V2";
const REBUILD_BATCH_SIZE: usize = 32;
const FAILED_REBUILD_SUMMARY: &str = "Embedding rebuild failed";

/// Serializable progress snapshot for the explicit BGE generation rebuild.
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
}

/// Read the current rebuild checkpoint without loading the embedding model.
///
/// A database created before migration 044 is represented as a legacy-ready
/// status, allowing diagnostics to remain available during initialization.
pub fn embedding_index_status(conn: &Connection) -> AppResult<EmbeddingIndexStatus> {
    let status = conn
        .query_row(
            "SELECT active_model_id, target_model_id, target_dimension, phase,
                    indexed_items, total_items, last_error
             FROM embedding_generation_state WHERE singleton = 1",
            [],
            |row| {
                Ok(EmbeddingIndexStatus {
                    active_model_id: row.get(0)?,
                    target_model_id: row.get(1)?,
                    dimension: row.get(2)?,
                    phase: row.get(3)?,
                    indexed_items: row.get(4)?,
                    total_items: row.get(5)?,
                    last_error: row.get(6)?,
                })
            },
        )
        .optional();
    match status {
        Ok(Some(status)) => Ok(status),
        Ok(None) => Ok(legacy_ready_status()),
        Err(error) if is_unavailable_embedding_schema(&error) => Ok(legacy_ready_status()),
        Err(error) => Err(error.into()),
    }
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
    }
}

fn is_unavailable_embedding_schema(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(_, Some(detail)) if detail.contains("no such table")
    )
}
/// A batch embedding provider used by the v2 generation rebuild.
///
/// The trait keeps the database state machine independently testable and makes
/// every rebuild go through the same dimension and coverage verification.
pub trait EmbeddingBatcher {
    /// Return exactly one embedding for every input text, in input order.
    fn embed_batch(&self, texts: &[&str]) -> AppResult<Vec<Vec<f32>>>;
}

struct BgeEmbeddingBatcher;

impl EmbeddingBatcher for BgeEmbeddingBatcher {
    fn embed_batch(&self, texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
        embed_texts_batch(texts)
    }
}

/// Rebuild every chunk into the active BGE v2 generation.
///
/// This is deliberately synchronous: the caller owns the explicit reindex
/// action and receives an error when the locally bundled model is unavailable.
/// The legacy table is never changed. Until this function atomically marks the
/// generation `ready`, semantic retrieval stays on non-vector fallbacks.
pub fn rebuild_v2_embeddings(conn: &Connection) -> AppResult<usize> {
    rebuild_v2_embeddings_with(conn, &BgeEmbeddingBatcher)
}

/// Rebuild v2 embeddings using an injected batcher.
///
/// Exposed for deterministic tests; production must call
/// [`rebuild_v2_embeddings`] so it only uses the verified bundled BGE model.
pub fn rebuild_v2_embeddings_with(
    conn: &Connection,
    batcher: &impl EmbeddingBatcher,
) -> AppResult<usize> {
    let chunks = load_chunk_snapshot(conn)?;
    let anchors = load_auxiliary_snapshot(conn, "semantic_anchors")?;
    let regulations = load_auxiliary_snapshot(conn, "regulation_index")?;
    let total = chunks.len() + anchors.len() + regulations.len();

    begin_rebuild(conn, total)?;

    let rebuild_result = rebuild_snapshot(conn, &chunks, batcher)
        .and_then(|()| {
            rebuild_auxiliary_snapshot(
                conn,
                "semantic_anchor_embeddings_v2",
                "anchor_id",
                &anchors,
                batcher,
            )
        })
        .and_then(|()| {
            rebuild_auxiliary_snapshot(
                conn,
                "regulation_embeddings_v2",
                "regulation_id",
                &regulations,
                batcher,
            )
        });
    if let Err(error) = rebuild_result {
        mark_failed(conn, total)?;
        return Err(error);
    }

    match finalize_ready(conn, &chunks, &anchors, &regulations) {
        Ok(()) => Ok(total),
        Err(error) => {
            mark_failed(conn, total)?;
            Err(error)
        }
    }
}

fn load_chunk_snapshot(conn: &Connection) -> AppResult<Vec<(i64, String)>> {
    let mut statement = conn.prepare("SELECT id, content FROM chunks ORDER BY id")?;
    let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.flatten().collect())
}

fn load_auxiliary_snapshot(conn: &Connection, source_table: &str) -> AppResult<Vec<(i64, String)>> {
    let sql = format!("SELECT id, content FROM {source_table} ORDER BY id");
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.flatten().collect())
}

fn rebuild_auxiliary_snapshot(
    conn: &Connection,
    target_table: &str,
    id_column: &str,
    records: &[(i64, String)],
    batcher: &impl EmbeddingBatcher,
) -> AppResult<()> {
    for batch in records.chunks(REBUILD_BATCH_SIZE) {
        let texts: Vec<&str> = batch.iter().map(|(_, text)| text.as_str()).collect();
        let embeddings = batcher.embed_batch(&texts)?;
        if embeddings.len() != batch.len()
            || embeddings
                .iter()
                .any(|item| item.len() != EMBEDDING_DIMENSION)
        {
            return Err(AppError::Embed(
                "Auxiliary embedding batch has invalid coverage or dimension".into(),
            ));
        }
        let sql = format!("INSERT INTO {target_table} ({id_column}, embedding) VALUES (?1, ?2) ON CONFLICT({id_column}) DO UPDATE SET embedding = excluded.embedding");
        for ((id, _), embedding) in batch.iter().zip(embeddings) {
            conn.execute(&sql, rusqlite::params![id, f32_to_bytes(&embedding)])?;
        }
    }
    Ok(())
}
fn begin_rebuild(conn: &Connection, total: usize) -> AppResult<()> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| -> AppResult<()> {
        conn.execute("DELETE FROM chunk_embeddings_v2", [])?;
        conn.execute("DELETE FROM semantic_anchor_embeddings_v2", [])?;
        conn.execute("DELETE FROM regulation_embeddings_v2", [])?;
        conn.execute(
            "UPDATE embedding_generation_state
             SET target_model_id = ?1,
                 target_dimension = ?2,
                 phase = 'rebuilding',
                 indexed_items = 0,
                 total_items = ?3,
                 last_error = NULL,
                 updated_at = datetime('now')
             WHERE singleton = 1",
            rusqlite::params![EMBEDDING_MODEL_ID, EMBEDDING_DIMENSION as i64, total as i64],
        )?;
        Ok(())
    })();
    finish_transaction(conn, result)
}

fn rebuild_snapshot(
    conn: &Connection,
    chunks: &[(i64, String)],
    batcher: &impl EmbeddingBatcher,
) -> AppResult<()> {
    let mut indexed = 0usize;
    for chunk_batch in chunks.chunks(REBUILD_BATCH_SIZE) {
        let texts = chunk_batch
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>();
        let embeddings = batcher.embed_batch(&texts)?;
        if embeddings.len() != chunk_batch.len() {
            return Err(AppError::Embed(format!(
                "Embedding batch returned {} vectors for {} chunks",
                embeddings.len(),
                chunk_batch.len()
            )));
        }
        if embeddings
            .iter()
            .any(|embedding| embedding.len() != EMBEDDING_DIMENSION)
        {
            return Err(AppError::Embed(format!(
                "Embedding batch returned a vector outside the required {EMBEDDING_DIMENSION} dimensions"
            )));
        }

        indexed += chunk_batch.len();
        store_batch_progress(conn, chunk_batch, &embeddings, indexed)?;
    }
    Ok(())
}

fn store_batch_progress(
    conn: &Connection,
    chunk_batch: &[(i64, String)],
    embeddings: &[Vec<f32>],
    indexed: usize,
) -> AppResult<()> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| -> AppResult<()> {
        for ((chunk_id, _), embedding) in chunk_batch.iter().zip(embeddings) {
            conn.execute(
                "INSERT INTO chunk_embeddings_v2 (chunk_id, embedding) VALUES (?1, ?2)
                 ON CONFLICT(chunk_id) DO UPDATE SET embedding = excluded.embedding",
                rusqlite::params![chunk_id, f32_to_bytes(embedding)],
            )?;
        }
        conn.execute(
            "UPDATE embedding_generation_state
             SET indexed_items = ?1, updated_at = datetime('now')
             WHERE singleton = 1 AND phase = 'rebuilding'",
            [indexed as i64],
        )?;
        Ok(())
    })();
    finish_transaction(conn, result)
}

fn finalize_ready(
    conn: &Connection,
    expected_chunks: &[(i64, String)],
    expected_anchors: &[(i64, String)],
    expected_regulations: &[(i64, String)],
) -> AppResult<()> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| -> AppResult<()> {
        let actual_chunks = load_chunk_snapshot(conn)?;
        if actual_chunks != expected_chunks {
            return Err(AppError::Embed(
                "Chunk set changed while embeddings were rebuilding".into(),
            ));
        }

        let total: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get(0))?;
        let embedded: i64 =
            conn.query_row("SELECT COUNT(*) FROM chunk_embeddings_v2", [], |row| {
                row.get(0)
            })?;
        let invalid_dimensions: i64 = conn.query_row(
            "SELECT COUNT(*) FROM chunk_embeddings_v2
             WHERE length(embedding) <> ?1",
            [((EMBEDDING_DIMENSION * std::mem::size_of::<f32>()) as i64)],
            |row| row.get(0),
        )?;
        let anchors: i64 = conn.query_row(
            "SELECT COUNT(*) FROM semantic_anchor_embeddings_v2",
            [],
            |row| row.get(0),
        )?;
        let regulations: i64 =
            conn.query_row("SELECT COUNT(*) FROM regulation_embeddings_v2", [], |row| {
                row.get(0)
            })?;
        if embedded != total
            || total != expected_chunks.len() as i64
            || invalid_dimensions != 0
            || anchors != expected_anchors.len() as i64
            || regulations != expected_regulations.len() as i64
        {
            return Err(AppError::Embed(
                "BGE v2 embedding coverage validation failed".into(),
            ));
        }

        conn.execute(
            "UPDATE embedding_generation_state
             SET active_model_id = ?1,
                 target_model_id = ?1,
                 target_dimension = ?2,
                 phase = 'ready',
                 indexed_items = ?3,
                 total_items = ?3,
                 last_error = NULL,
                 updated_at = datetime('now')
             WHERE singleton = 1",
            rusqlite::params![
                EMBEDDING_MODEL_ID,
                EMBEDDING_DIMENSION as i64,
                (expected_chunks.len() + expected_anchors.len() + expected_regulations.len())
                    as i64,
            ],
        )?;
        Ok(())
    })();
    finish_transaction(conn, result)
}

fn mark_failed(conn: &Connection, total: usize) -> AppResult<()> {
    conn.execute(
        "UPDATE embedding_generation_state
         SET active_model_id = CASE
                 WHEN active_model_id = ?1 THEN ?2
                 ELSE active_model_id
             END,
             target_model_id = ?1,
             target_dimension = ?3,
             phase = 'failed',
             total_items = ?4,
             last_error = ?5,
             updated_at = datetime('now')
         WHERE singleton = 1",
        rusqlite::params![
            EMBEDDING_MODEL_ID,
            LEGACY_MODEL_ID,
            EMBEDDING_DIMENSION as i64,
            total as i64,
            FAILED_REBUILD_SUMMARY,
        ],
    )?;
    Ok(())
}

fn finish_transaction(conn: &Connection, result: AppResult<()>) -> AppResult<()> {
    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT")?;
            Ok(())
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}
