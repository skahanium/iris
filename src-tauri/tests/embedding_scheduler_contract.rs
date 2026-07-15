use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use iris_lib::embedding::engine::{embedding_generation_ready, EMBEDDING_DIMENSION};
use iris_lib::embedding::scheduler::{
    recover_interrupted_generation, EmbeddingBatcher, EmbeddingScheduler, EmbeddingStartResult,
    EmbeddingStartSource,
};
use iris_lib::error::{AppError, AppResult};
use iris_lib::storage::db::Database;
use iris_lib::storage::migrate::migrate_up;
use rusqlite::Connection;

struct ScriptedBatcher {
    results: Mutex<VecDeque<AppResult<Vec<Vec<f32>>>>>,
}

impl ScriptedBatcher {
    fn new(results: Vec<AppResult<Vec<Vec<f32>>>>) -> Self {
        Self {
            results: Mutex::new(results.into()),
        }
    }
}

impl EmbeddingBatcher for ScriptedBatcher {
    fn ensure_available(&self) -> AppResult<()> {
        Ok(())
    }

    fn embed_batch(&self, _texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
        self.results
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err(AppError::Embed("unexpected batch".into())))
    }
}

fn seed_chunk(conn: &Connection, content_hash: &str) {
    conn.execute(
        "INSERT INTO files(path,title,content_hash,word_count,created_at,updated_at)
         VALUES ('note.md','Note','file',1,'now','now')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chunks(file_id,chunk_index,content,content_hash)
         VALUES (1,0,'body',?1)",
        [content_hash],
    )
    .unwrap();
}

#[test]
fn startup_marks_running_generation_interrupted_without_retrying_it() {
    let conn = Connection::open_in_memory().expect("open database");
    migrate_up(&conn).expect("migrate database");
    conn.execute(
        "UPDATE embedding_generation_state
         SET phase = 'running', indexed_items = 2, total_items = 8",
        [],
    )
    .expect("seed abandoned job");

    recover_interrupted_generation(&conn).expect("recover interrupted job");

    let state: (String, String, String) = conn
        .query_row(
            "SELECT phase, failure_code, last_error FROM embedding_generation_state",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("read recovered state");
    assert_eq!(state.0, "failed");
    assert_eq!(state.1, "interrupted_restart");
    assert_eq!(state.2, "Embedding rebuild interrupted");
}

#[test]
fn ready_phase_requires_matching_source_fingerprint_coverage() {
    let conn = Connection::open_in_memory().unwrap();
    migrate_up(&conn).unwrap();
    seed_chunk(&conn, "fingerprint-a");
    conn.execute(
        "INSERT INTO chunk_embeddings_v2(chunk_id, embedding, source_fingerprint, model_id, dimension)
         VALUES (1, zeroblob(?1), 'stale', 'Xenova/bge-small-zh-v1.5', 512)",
        [(EMBEDDING_DIMENSION * std::mem::size_of::<f32>()) as i64],
    ).unwrap();
    conn.execute(
        "UPDATE embedding_generation_state SET active_model_id='Xenova/bge-small-zh-v1.5',
         target_model_id='Xenova/bge-small-zh-v1.5', target_dimension=512,
         phase='ready', indexed_items=1, total_items=1",
        [],
    )
    .unwrap();

    assert!(!embedding_generation_ready(&conn).unwrap());
}

#[test]
fn scheduler_coalesces_duplicate_start_and_marks_valid_coverage_ready() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    db.with_conn(|conn| {
        seed_chunk(conn, "chunk-hash");
        Ok(())
    })
    .unwrap();
    let batcher = Arc::new(ScriptedBatcher::new(vec![Ok(vec![
        vec![0.25; EMBEDDING_DIMENSION],
    ])]));
    let scheduler = EmbeddingScheduler::with_batcher(Arc::clone(&db), batcher);
    scheduler.set_foreground_busy(false);

    assert_eq!(
        scheduler
            .start_generation(EmbeddingStartSource::Manual)
            .unwrap(),
        EmbeddingStartResult::Started
    );
    assert_eq!(
        scheduler
            .start_generation(EmbeddingStartSource::Manual)
            .unwrap(),
        EmbeddingStartResult::AlreadyRunning
    );

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if scheduler.status().unwrap().phase == "ready" {
            break;
        }
        assert!(Instant::now() < deadline, "scheduler did not finish");
        thread::sleep(Duration::from_millis(10));
    }
    db.with_read_conn(|conn| {
        assert!(embedding_generation_ready(conn)?);
        let metadata: (String, String, i64) = conn.query_row(
            "SELECT source_fingerprint, model_id, dimension FROM chunk_embeddings_v2 WHERE chunk_id = 1", [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(metadata, ("chunk-hash".into(), "Xenova/bge-small-zh-v1.5".into(), 512));
        Ok(())
    }).unwrap();
}
