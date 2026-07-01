use std::collections::HashSet;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, Weak};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::app::AppState;
use crate::error::AppResult;

// Re-export the consolidated WriteGuard from cas module
pub use crate::cas::write_guard::WriteGuard;

const EMBED_QUEUE_CAPACITY: usize = 256;

/// Background worker for chunk embeddings so IPC and the file watcher stay responsive.
pub struct EmbedQueue {
    tx: SyncSender<i64>,
    _worker: JoinHandle<()>,
    pending: Arc<Mutex<HashSet<i64>>>,
}

impl EmbedQueue {
    pub fn spawn(state: Arc<AppState>) -> Self {
        let (tx, rx) = mpsc::sync_channel(EMBED_QUEUE_CAPACITY);
        let pending = Arc::new(Mutex::new(HashSet::new()));
        let pending_worker = pending.clone();
        let worker = thread::Builder::new()
            .name("iris-embed".into())
            .spawn(move || {
                // Downgrade the strong Arc to a Weak BEFORE entering the worker
                // loop and drop the strong reference. Otherwise the move closure
                // would keep a strong `Arc<AppState>` alive for the entire
                // lifetime of the worker thread (which blocks on `rx.recv()`),
                // pinning `AppState` in memory even after all other holders drop
                // it — both a test invariant violation and a production leak on
                // app shutdown.
                let weak = Arc::downgrade(&state);
                drop(state);
                embed_worker_loop(weak, rx, pending_worker)
            })
            .expect("embed worker thread");

        Self {
            tx,
            _worker: worker,
            pending,
        }
    }

    /// Queue embedding for a file; duplicate file ids are coalesced while pending.
    pub fn enqueue(&self, file_id: i64) {
        let mut guard = self.pending.lock().expect("embed pending lock");
        if !guard.insert(file_id) {
            return;
        }
        drop(guard);
        if self.tx.try_send(file_id).is_err() {
            tracing::debug!(file_id, "embed queue full, will retry on next index");
            let mut guard = self.pending.lock().expect("embed pending lock");
            guard.remove(&file_id);
        }
    }
}

fn embed_worker_loop(state: Weak<AppState>, rx: Receiver<i64>, pending: Arc<Mutex<HashSet<i64>>>) {
    while let Ok(first_id) = rx.recv() {
        let Some(state) = state.upgrade() else {
            break;
        };
        let mut batch = vec![first_id];
        {
            let mut guard = pending.lock().expect("embed pending lock");
            guard.remove(&first_id);
        }
        // Drain up to 15 more file_ids within 80ms for batch embedding
        let deadline = std::time::Instant::now() + Duration::from_millis(80);
        while batch.len() < 16 {
            match rx.try_recv() {
                Ok(id) => {
                    {
                        let mut guard = pending.lock().expect("embed pending lock");
                        guard.remove(&id);
                    }
                    batch.push(id);
                    if std::time::Instant::now() >= deadline {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        for &file_id in &batch {
            if let Err(e) = embed_file_chunked(&state, file_id) {
                tracing::warn!("background embedding failed for file {file_id}: {e}");
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
}

/// Embed chunks for a file while minimizing DB write-lock hold time.
///
/// 1. Read chunks (brief read lock)
/// 2. Compute embeddings in batch (no lock — this is the expensive part)
/// 3. Write embeddings (brief write lock)
fn embed_file_chunked(state: &Arc<AppState>, file_id: i64) -> AppResult<()> {
    use super::engine::{embed_texts_batch, f32_to_bytes};
    use crate::storage::db;

    let chunks: Vec<(i64, String)> = state.db.with_read_conn(|conn| {
        let mut stmt =
            conn.prepare("SELECT id, content FROM chunks WHERE file_id = ?1 ORDER BY chunk_index")?;
        let rows = stmt.query_map([file_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        Ok(rows.flatten().collect())
    })?;

    if chunks.is_empty() {
        return Ok(());
    }

    let texts: Vec<&str> = chunks.iter().map(|(_, c)| c.as_str()).collect();
    let batch_results = match embed_texts_batch(&texts) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("batch embedding failed for file {file_id}: {e}");
            return Ok(());
        }
    };

    let embeddings: Vec<(i64, Vec<u8>)> = chunks
        .iter()
        .zip(batch_results.iter())
        .map(|((chunk_id, _), vec)| (*chunk_id, f32_to_bytes(vec)))
        .collect();

    if embeddings.is_empty() {
        return Ok(());
    }

    state.db.with_conn(|conn| {
        for (chunk_id, blob) in &embeddings {
            conn.execute(
                "INSERT OR REPLACE INTO chunk_embeddings (chunk_id, embedding) VALUES (?1, ?2)",
                rusqlite::params![chunk_id, blob],
            )?;
            if db::vector_index_ready() {
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO vec_chunks (rowid, embedding) VALUES (?1, ?2)",
                    rusqlite::params![chunk_id, blob],
                );
            }
        }
        Ok(())
    })
}
