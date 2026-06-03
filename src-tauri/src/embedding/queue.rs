use std::collections::HashSet;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::app::AppState;
use crate::error::AppResult;

// Re-export the consolidated WriteGuard from cas module
pub use crate::cas::write_guard::WriteGuard;

/// Background worker for chunk embeddings so IPC and the file watcher stay responsive.
pub struct EmbedQueue {
    tx: Sender<i64>,
    _worker: JoinHandle<()>,
    pending: Arc<Mutex<HashSet<i64>>>,
}

impl EmbedQueue {
    pub fn spawn(state: Arc<AppState>) -> Self {
        let (tx, rx) = mpsc::channel();
        let pending = Arc::new(Mutex::new(HashSet::new()));
        let pending_worker = pending.clone();
        let worker = thread::Builder::new()
            .name("iris-embed".into())
            .spawn(move || embed_worker_loop(state, rx, pending_worker))
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
        let _ = self.tx.send(file_id);
    }
}

fn embed_worker_loop(state: Arc<AppState>, rx: Receiver<i64>, pending: Arc<Mutex<HashSet<i64>>>) {
    while let Ok(file_id) = rx.recv() {
        {
            let mut guard = pending.lock().expect("embed pending lock");
            guard.remove(&file_id);
        }
        if let Err(e) = embed_file_chunked(&state, file_id) {
            tracing::warn!("background embedding failed for file {file_id}: {e}");
        }
        thread::sleep(Duration::from_millis(5));
    }
}

/// Embed chunks for a file while minimizing DB write‐lock hold time.
///
/// 1. Read chunks (brief lock)
/// 2. Compute embeddings (no lock — this is the expensive part)
/// 3. Write embeddings (brief lock)
fn embed_file_chunked(state: &Arc<AppState>, file_id: i64) -> AppResult<()> {
    use super::engine::{embed_text, f32_to_bytes};
    use crate::storage::db;

    let chunks: Vec<(i64, String)> = state.db.with_conn(|conn| {
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

    let mut embeddings: Vec<(i64, Vec<u8>)> = Vec::with_capacity(chunks.len());
    for (chunk_id, content) in &chunks {
        match embed_text(content) {
            Ok(v) => embeddings.push((*chunk_id, f32_to_bytes(&v))),
            Err(e) => tracing::warn!("chunk {chunk_id} embedding skipped: {e}"),
        }
    }

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
