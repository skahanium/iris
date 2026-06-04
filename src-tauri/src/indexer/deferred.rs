//! Coalesces background `index_file_from_content` jobs per note path.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::app::AppState;
use crate::indexer::scan::index_file_from_content;
const INDEX_DEBOUNCE_MS: u64 = 2500;
const MAX_PENDING_ENTRIES: usize = 50;

struct PendingIndex {
    generation: u64,
    path: String,
    content: String,
    hash: String,
    abs: PathBuf,
    vault: PathBuf,
}

/// Per-path debounced indexer: rapid `file_write` calls merge into one background index.
#[derive(Default)]
pub struct DeferredFileIndexer {
    pending: Mutex<HashMap<String, PendingIndex>>,
}

impl DeferredFileIndexer {
    /// Flush the oldest pending entry immediately when the map exceeds capacity.
    fn flush_oldest_if_full(
        guard: &mut HashMap<String, PendingIndex>,
        state: &Arc<AppState>,
    ) {
        if guard.len() < MAX_PENDING_ENTRIES {
            return;
        }
        let oldest_key = guard
            .iter()
            .min_by_key(|(_, v)| v.generation)
            .map(|(k, _)| k.clone());
        if let Some(key) = oldest_key {
            if let Some(job) = guard.remove(&key) {
                let state = state.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = tokio::task::spawn_blocking(move || {
                        state.db.with_conn(|conn| {
                            index_file_from_content(
                                conn,
                                &job.vault,
                                &job.abs,
                                &job.content,
                                &job.hash,
                                Some(&state),
                            )
                        })
                    })
                    .await;
                });
            }
        }
    }

    pub fn schedule(
        indexer: Arc<Self>,
        state: Arc<AppState>,
        path: String,
        content: String,
        hash: String,
        abs: PathBuf,
        vault: PathBuf,
    ) {
        let generation = {
            let mut guard = indexer.pending.lock().expect("deferred index lock");
            Self::flush_oldest_if_full(&mut guard, &state);
            let next = guard
                .get(&path)
                .map(|p| p.generation.saturating_add(1))
                .unwrap_or(1);
            guard.insert(
                path.clone(),
                PendingIndex {
                    generation: next,
                    path: path.clone(),
                    content,
                    hash,
                    abs,
                    vault,
                },
            );
            next
        };

        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(INDEX_DEBOUNCE_MS)).await;

            let job = {
                let guard = indexer.pending.lock().expect("deferred index lock");
                guard.get(&path).cloned()
            };

            let Some(job) = job else {
                return;
            };

            if job.generation != generation {
                return;
            }

            {
                let mut guard = indexer.pending.lock().expect("deferred index lock");
                if guard.get(&path).map(|p| p.generation) != Some(generation) {
                    return;
                }
                guard.remove(&path);
            }

            let result = tokio::task::spawn_blocking(move || {
                state.db.with_conn(|conn| {
                    index_file_from_content(
                        conn,
                        &job.vault,
                        &job.abs,
                        &job.content,
                        &job.hash,
                        Some(&state),
                    )
                })
            })
            .await;

            match result {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    tracing::warn!(path = %job.path, "deferred index failed: {e}");
                }
                Err(e) => tracing::warn!(path = %job.path, "deferred index join failed: {e}"),
            }
        });
    }
}

impl Clone for PendingIndex {
    fn clone(&self) -> Self {
        Self {
            generation: self.generation,
            path: self.path.clone(),
            content: self.content.clone(),
            hash: self.hash.clone(),
            abs: self.abs.clone(),
            vault: self.vault.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debounce_constant_is_two_and_half_seconds() {
        assert_eq!(INDEX_DEBOUNCE_MS, 2500);
    }
}
