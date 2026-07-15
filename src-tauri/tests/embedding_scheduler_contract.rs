use std::collections::VecDeque;
use std::sync::mpsc;
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

struct UnavailableBatcher;

impl EmbeddingBatcher for UnavailableBatcher {
    fn ensure_available(&self) -> AppResult<()> {
        Err(AppError::Embed("missing /secret/model".into()))
    }
    fn embed_batch(&self, _texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
        unreachable!("preflight must stop job")
    }
}

struct BlockingBatcher {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl EmbeddingBatcher for BlockingBatcher {
    fn ensure_available(&self) -> AppResult<()> {
        Ok(())
    }

    fn embed_batch(&self, texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
        self.entered.send(()).unwrap();
        self.release.lock().unwrap().recv().unwrap();
        Ok(vec![vec![0.3; EMBEDDING_DIMENSION]; texts.len()])
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

fn seed_chunks(conn: &Connection, count: usize) {
    conn.execute(
        "INSERT INTO files(path,title,content_hash,word_count,created_at,updated_at)
         VALUES ('many.md','Many','file',1,'now','now')",
        [],
    )
    .unwrap();
    for index in 0..count {
        conn.execute(
            "INSERT INTO chunks(file_id,chunk_index,content,content_hash)
             VALUES (1,?1,?2,?3)",
            rusqlite::params![
                index as i64,
                format!("body-{index}"),
                format!("hash-{index}")
            ],
        )
        .unwrap();
    }
}

fn wait_for_phase(scheduler: &EmbeddingScheduler, phase: &str) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if scheduler.status().unwrap().phase == phase {
            return;
        }
        assert!(Instant::now() < deadline, "scheduler did not reach {phase}");
        thread::sleep(Duration::from_millis(10));
    }
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

    wait_for_phase(&scheduler, "ready");
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

#[test]
fn failed_generation_preserves_valid_batch_and_manual_restart_only_embeds_gap() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    db.with_conn(|conn| {
        seed_chunks(conn, 17);
        Ok(())
    })
    .unwrap();
    let first_batcher = Arc::new(ScriptedBatcher::new(vec![
        Ok(vec![vec![0.1; EMBEDDING_DIMENSION]; 16]),
        Err(AppError::Embed(
            "model raw failure /vault/private.md".into(),
        )),
    ]));
    let first = EmbeddingScheduler::with_batcher(Arc::clone(&db), first_batcher);
    first.set_foreground_busy(false);
    first
        .start_generation(EmbeddingStartSource::Manual)
        .unwrap();
    wait_for_phase(&first, "failed");
    db.with_read_conn(|conn| {
        let indexed: i64 =
            conn.query_row("SELECT COUNT(*) FROM chunk_embeddings_v2", [], |row| {
                row.get(0)
            })?;
        assert_eq!(indexed, 16);
        let error: String = conn.query_row(
            "SELECT last_error FROM embedding_generation_state",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(error, "Embedding rebuild failed");
        assert!(!error.contains("private"));
        Ok(())
    })
    .unwrap();

    let resume_batcher = Arc::new(ScriptedBatcher::new(vec![Ok(vec![
        vec![0.2; EMBEDDING_DIMENSION],
    ])]));
    let resumed = EmbeddingScheduler::with_batcher(Arc::clone(&db), resume_batcher);
    resumed.set_foreground_busy(false);
    resumed
        .start_generation(EmbeddingStartSource::Manual)
        .unwrap();
    wait_for_phase(&resumed, "ready");
    db.with_read_conn(|conn| {
        let indexed: i64 =
            conn.query_row("SELECT COUNT(*) FROM chunk_embeddings_v2", [], |row| {
                row.get(0)
            })?;
        assert_eq!(indexed, 17);
        Ok(())
    })
    .unwrap();
}

#[test]
fn unavailable_model_marks_safe_failure_without_touching_document_index() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    db.with_conn(|conn| {
        seed_chunk(conn, "chunk-hash");
        Ok(())
    })
    .unwrap();
    let scheduler = EmbeddingScheduler::with_batcher(Arc::clone(&db), Arc::new(UnavailableBatcher));
    scheduler.set_foreground_busy(false);
    scheduler
        .start_generation(EmbeddingStartSource::Manual)
        .unwrap();
    wait_for_phase(&scheduler, "failed");
    db.with_read_conn(|conn| {
        let state: (String, String, i64) = conn.query_row(
            "SELECT failure_code, last_error, (SELECT COUNT(*) FROM chunks) FROM embedding_generation_state", [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(state.0, "model_unavailable");
        assert_eq!(state.1, "Embedding model unavailable");
        assert_eq!(state.2, 1);
        Ok(())
    }).unwrap();
}

#[test]
fn blocked_model_batch_does_not_hold_database_connection() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    db.with_conn(|conn| {
        seed_chunk(conn, "chunk-hash");
        Ok(())
    })
    .unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let scheduler = EmbeddingScheduler::with_batcher(
        Arc::clone(&db),
        Arc::new(BlockingBatcher {
            entered: entered_tx,
            release: Mutex::new(release_rx),
        }),
    );
    scheduler.set_foreground_busy(false);
    scheduler
        .start_generation(EmbeddingStartSource::Manual)
        .unwrap();
    entered_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("model batch entered");

    let started = Instant::now();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO settings(key, value) VALUES ('write_during_model', 'ok')",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    assert!(
        started.elapsed() < Duration::from_millis(250),
        "write waited for model inference"
    );

    release_tx.send(()).unwrap();
    wait_for_phase(&scheduler, "ready");
}

#[test]
fn idle_resumes_ready_repair_after_enqueue_without_manual_pause() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    db.with_conn(|conn| {
        seed_chunk(conn, "chunk-hash");
        conn.execute(
            "UPDATE embedding_generation_state
             SET active_model_id = 'Xenova/bge-small-zh-v1.5',
                 target_model_id = 'Xenova/bge-small-zh-v1.5',
                 target_dimension = 512, phase = 'ready'",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    let scheduler = EmbeddingScheduler::with_batcher(
        Arc::clone(&db),
        Arc::new(ScriptedBatcher::new(vec![Ok(vec![
            vec![0.4; EMBEDDING_DIMENSION],
        ])])),
    );
    scheduler.set_foreground_busy(false);
    scheduler.mark_initial_index_complete();
    scheduler.enqueue_file(1);
    assert_eq!(scheduler.status().unwrap().phase, "paused");

    assert_eq!(
        scheduler
            .start_generation(EmbeddingStartSource::Automatic)
            .unwrap(),
        EmbeddingStartResult::Started
    );
    wait_for_phase(&scheduler, "ready");
}

#[test]
fn manual_pause_resume_restarts_paused_job_when_idle() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    db.with_conn(|conn| {
        seed_chunk(conn, "chunk-hash");
        conn.execute(
            "UPDATE embedding_generation_state SET phase = 'paused' WHERE singleton = 1",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    let scheduler = EmbeddingScheduler::with_batcher(
        Arc::clone(&db),
        Arc::new(ScriptedBatcher::new(vec![Ok(vec![
            vec![0.5; EMBEDDING_DIMENSION],
        ])])),
    );
    scheduler.set_foreground_busy(false);
    scheduler.mark_initial_index_complete();
    scheduler.set_manual_paused(true).unwrap();
    scheduler.set_manual_paused(false).unwrap();

    wait_for_phase(&scheduler, "ready");
}

#[test]
fn vault_reset_during_model_inference_prevents_old_batch_commit() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    db.with_conn(|conn| {
        seed_chunk(conn, "chunk-hash");
        Ok(())
    })
    .unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let scheduler = EmbeddingScheduler::with_batcher(
        Arc::clone(&db),
        Arc::new(BlockingBatcher {
            entered: entered_tx,
            release: Mutex::new(release_rx),
        }),
    );
    scheduler.set_foreground_busy(false);
    scheduler
        .start_generation(EmbeddingStartSource::Manual)
        .unwrap();
    entered_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("model batch entered");

    scheduler.reset_for_vault();
    release_tx.send(()).unwrap();
    thread::sleep(Duration::from_millis(100));

    db.with_read_conn(|conn| {
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM chunk_embeddings_v2", [], |row| {
            row.get(0)
        })?;
        assert_eq!(count, 0, "old-vault model output must never be committed");
        Ok(())
    })
    .unwrap();
}
