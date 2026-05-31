use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::app::AppState;
use crate::embedding::store::store_chunk_embeddings;
use crate::error::AppResult;

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
        let result: AppResult<()> = state.db.with_conn(|conn| {
            store_chunk_embeddings(conn, file_id)?;
            Ok(())
        });
        if let Err(e) = result {
            tracing::warn!("background embedding failed for file {file_id}: {e}");
        }
        // Brief yield so burst saves do not monopolize a core.
        thread::sleep(Duration::from_millis(5));
    }
}

/// Recent app-initiated writes: watcher skips re-index when hash matches within TTL.
#[derive(Default)]
pub struct WriteGuard {
    entries: Mutex<HashMap<String, (String, std::time::Instant)>>,
}

impl WriteGuard {
    const TTL: Duration = Duration::from_secs(3);

    pub fn mark(&self, relative_path: &str, content_hash: &str) {
        let mut guard = self.entries.lock().expect("write guard lock");
        guard.insert(
            relative_path.to_string(),
            (content_hash.to_string(), std::time::Instant::now()),
        );
    }

    pub fn should_skip_watcher(&self, relative_path: &str, content_hash: &str) -> bool {
        let mut guard = self.entries.lock().expect("write guard lock");
        guard.retain(|_, (_, t)| t.elapsed() < Self::TTL);
        guard
            .get(relative_path)
            .is_some_and(|(h, t)| h == content_hash && t.elapsed() < Self::TTL)
    }
}
