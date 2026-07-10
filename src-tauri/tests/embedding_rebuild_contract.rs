use std::collections::VecDeque;
use std::sync::Mutex;

use iris_lib::embedding::engine::{
    embedding_generation_ready, EMBEDDING_DIMENSION, EMBEDDING_MODEL_ID,
};
use iris_lib::embedding::rebuild::{
    embedding_index_status, rebuild_v2_embeddings_with, EmbeddingBatcher,
};
use iris_lib::error::{AppError, AppResult};
use iris_lib::storage::migrate::migrate_up;
use rusqlite::Connection;

struct ScriptedBatcher {
    responses: Mutex<VecDeque<AppResult<Vec<Vec<f32>>>>>,
}

impl ScriptedBatcher {
    fn new(responses: Vec<AppResult<Vec<Vec<f32>>>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
        }
    }
}

impl EmbeddingBatcher for ScriptedBatcher {
    fn embed_batch(&self, _texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
        self.responses
            .lock()
            .expect("batcher responses lock")
            .pop_front()
            .expect("unexpected batch")
    }
}

fn migrated_connection() -> Connection {
    let conn = Connection::open_in_memory().expect("open database");
    migrate_up(&conn).expect("migrate database");
    conn
}

fn seed_chunks(conn: &Connection, count: usize) {
    conn.execute(
        "INSERT INTO files (path, title, content_hash, word_count, created_at, updated_at)
         VALUES ('notes/rebuild.md', 'Rebuild', 'hash', 1, datetime('now'), datetime('now'))",
        [],
    )
    .expect("insert file");
    for index in 0..count {
        conn.execute(
            "INSERT INTO chunks (file_id, chunk_index, content) VALUES (1, ?1, ?2)",
            rusqlite::params![index as i64, format!("chunk {index}")],
        )
        .expect("insert chunk");
    }
}

fn generation_state(conn: &Connection) -> (String, String, i64, i64, String, Option<String>) {
    conn.query_row(
        "SELECT active_model_id, phase, indexed_items, total_items, target_model_id, last_error
         FROM embedding_generation_state WHERE singleton = 1",
        [],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        },
    )
    .expect("read generation state")
}

#[test]
fn rebuild_marks_v2_ready_only_after_complete_512_dimensional_coverage() {
    let conn = migrated_connection();
    seed_chunks(&conn, 2);
    conn.execute(
        "INSERT INTO chunk_embeddings (chunk_id, embedding) VALUES (1, x'01020304')",
        [],
    )
    .expect("seed legacy embedding");
    let embedding = vec![0.5_f32; EMBEDDING_DIMENSION];
    let batcher = ScriptedBatcher::new(vec![Ok(vec![embedding.clone(), embedding])]);

    let rebuilt = rebuild_v2_embeddings_with(&conn, &batcher).expect("rebuild v2 embeddings");

    assert_eq!(rebuilt, 2);
    let state = generation_state(&conn);
    assert_eq!(state.0, EMBEDDING_MODEL_ID);
    assert_eq!(state.1, "ready");
    assert_eq!(state.2, 2);
    assert_eq!(state.3, 2);
    assert_eq!(state.4, EMBEDDING_MODEL_ID);
    assert_eq!(state.5, None);
    assert!(embedding_generation_ready(&conn).expect("ready v2 state should be searchable"));
    let v2_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM chunk_embeddings_v2", [], |row| {
            row.get(0)
        })
        .expect("count rebuilt embeddings");
    let v2_bytes: i64 = conn
        .query_row(
            "SELECT length(embedding) FROM chunk_embeddings_v2 WHERE chunk_id = 1",
            [],
            |row| row.get(0),
        )
        .expect("read rebuilt embedding");
    let legacy: Vec<u8> = conn
        .query_row(
            "SELECT embedding FROM chunk_embeddings WHERE chunk_id = 1",
            [],
            |row| row.get(0),
        )
        .expect("read legacy embedding");
    assert_eq!(v2_count, 2);
    assert_eq!(
        v2_bytes,
        (EMBEDDING_DIMENSION * std::mem::size_of::<f32>()) as i64
    );
    assert_eq!(legacy, vec![1, 2, 3, 4]);
}

#[test]
fn rebuild_with_invalid_dimension_never_activates_v2() {
    let conn = migrated_connection();
    seed_chunks(&conn, 1);
    let batcher = ScriptedBatcher::new(vec![Ok(vec![vec![0.5_f32; EMBEDDING_DIMENSION - 1]])]);

    let error = rebuild_v2_embeddings_with(&conn, &batcher).expect_err("invalid vectors must fail");

    assert!(matches!(error, AppError::Embed(_)));
    let state = generation_state(&conn);
    assert_eq!(state.0, "fastembed/AllMiniLML6V2");
    assert_eq!(state.1, "failed");
    assert_eq!(state.2, 0);
    assert_eq!(state.3, 1);
    assert!(state.5.is_some());
    assert!(!embedding_generation_ready(&conn).expect("failed rebuild must stay unavailable"));
    let v2_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM chunk_embeddings_v2", [], |row| {
            row.get(0)
        })
        .expect("count failed rebuild embeddings");
    assert_eq!(v2_count, 0);
}

#[test]
fn rebuild_failure_persists_only_sanitized_error_summary() {
    let conn = migrated_connection();
    seed_chunks(&conn, 1);
    let batcher = ScriptedBatcher::new(vec![Err(AppError::Embed(
        "failed on /private/vault/secret.md with sk-very-secret-token".into(),
    ))]);

    let _ =
        rebuild_v2_embeddings_with(&conn, &batcher).expect_err("backend failure must propagate");

    let state = generation_state(&conn);
    assert_eq!(state.1, "failed");
    let last_error = state.5.expect("failed state records diagnostic summary");
    assert_eq!(last_error, "Embedding rebuild failed");
    assert!(!last_error.contains("secret"));
    assert_ne!(state.1, "ready");
}
#[test]
fn status_degrades_to_legacy_ready_before_generation_migration_exists() {
    let conn = Connection::open_in_memory().expect("open unmigrated database");

    let status = embedding_index_status(&conn).expect("read degraded status");

    assert_eq!(status.phase, "legacy_ready");
    assert_eq!(status.active_model_id, "fastembed/AllMiniLML6V2");
    assert_eq!(status.target_model_id, EMBEDDING_MODEL_ID);
    assert_eq!(status.dimension, EMBEDDING_DIMENSION as i64);
    assert_eq!(status.indexed_items, 0);
    assert_eq!(status.total_items, 0);
    assert_eq!(status.last_error, None);
}

#[test]
fn partial_rebuild_never_activates_v2_after_a_later_batch_fails() {
    let conn = migrated_connection();
    seed_chunks(&conn, 33);
    let valid_batch = vec![vec![0.5_f32; EMBEDDING_DIMENSION]; 32];
    let batcher = ScriptedBatcher::new(vec![
        Ok(valid_batch),
        Err(AppError::Embed("second batch failed".into())),
    ]);

    let error = rebuild_v2_embeddings_with(&conn, &batcher)
        .expect_err("a partial generation must not be activated");

    assert!(matches!(error, AppError::Embed(_)));
    let state = generation_state(&conn);
    assert_eq!(state.0, "fastembed/AllMiniLML6V2");
    assert_eq!(state.1, "failed");
    assert_eq!(state.2, 32);
    assert_eq!(state.3, 33);
    assert!(!embedding_generation_ready(&conn).expect("partial generation must stay unavailable"));
    let v2_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM chunk_embeddings_v2", [], |row| {
            row.get(0)
        })
        .expect("count partial generation");
    assert_eq!(v2_count, 32);
}

#[test]
fn v2_generation_owns_separate_anchor_and_regulation_embedding_tables() {
    let conn = migrated_connection();
    for table in ["semantic_anchor_embeddings_v2", "regulation_embeddings_v2"] {
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .expect("query table");
        assert_eq!(exists, 1, "{table} must be migrated");
    }
}
#[test]
fn rebuild_v2_embeds_structured_anchor_and_regulation_records() {
    let conn = migrated_connection();
    seed_chunks(&conn, 1);
    conn.execute(
        "INSERT INTO semantic_anchors
         (anchor_key, file_id, anchor_type, content, source_start, source_end, content_hash,
          extractor_version, embedding_model, embedding_dim, confidence, created_at, updated_at)
         VALUES ('a1', 1, 'claim', 'anchor body', 0, 11, 'ah', 'v1', 'legacy', 384, 1.0, 'now', 'now')",
        [],
    ).expect("seed anchor");
    conn.execute(
        "INSERT INTO regulation_index
         (file_id, regulation_name, article, content, source_start, source_end, content_hash,
          parser_version, embedding_model, embedding_dim, created_at)
         VALUES (1, 'Rule', '1', 'regulation body', 0, 15, 'rh', 'v1', 'legacy', 384, 'now')",
        [],
    )
    .expect("seed regulation");
    let embedding = vec![0.25_f32; EMBEDDING_DIMENSION];
    let batcher = ScriptedBatcher::new(vec![
        Ok(vec![embedding.clone()]),
        Ok(vec![embedding.clone()]),
        Ok(vec![embedding]),
    ]);

    rebuild_v2_embeddings_with(&conn, &batcher).expect("rebuild all v2 records");

    for table in [
        "chunk_embeddings_v2",
        "semantic_anchor_embeddings_v2",
        "regulation_embeddings_v2",
    ] {
        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("count v2 rows");
        assert_eq!(count, 1, "{table}");
    }
}

#[test]
fn corrupted_auxiliary_v2_vector_prevents_generation_readiness() {
    let conn = migrated_connection();
    seed_chunks(&conn, 1);
    conn.execute(
        "INSERT INTO semantic_anchors
         (anchor_key, file_id, anchor_type, content, source_start, source_end, content_hash,
          extractor_version, embedding_model, embedding_dim, confidence, created_at, updated_at)
         VALUES ('a-ready', 1, 'claim', 'anchor body', 0, 11, 'ah', 'v1', 'legacy', 384, 1.0, 'now', 'now')",
        [],
    ).expect("seed anchor");
    let embedding = vec![0.25_f32; EMBEDDING_DIMENSION];
    let batcher = ScriptedBatcher::new(vec![Ok(vec![embedding.clone()]), Ok(vec![embedding])]);
    rebuild_v2_embeddings_with(&conn, &batcher).expect("rebuild v2");
    assert!(embedding_generation_ready(&conn).expect("ready before corruption"));

    conn.execute(
        "UPDATE semantic_anchor_embeddings_v2 SET embedding = x'0102'",
        [],
    )
    .expect("corrupt auxiliary vector");

    assert!(!embedding_generation_ready(&conn).expect("corrupt auxiliary must block readiness"));
}
