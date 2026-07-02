use std::sync::Arc;

use chrono::Utc;
use tokio::sync::watch;
use tokio::time::{sleep, Duration};

use crate::ai_runtime::session::SessionManager;
use crate::app::AppState;
use crate::cas::garbage_collector::GarbageCollector;
use crate::error::AppResult;

/// Periodic background task scheduler.
pub struct Scheduler {
    state: Arc<AppState>,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

/// Handle held by `lib.rs` for scheduler lifetime; Drop sends the shutdown signal.
pub struct ShutdownHandle {
    #[allow(dead_code)]
    tx: watch::Sender<bool>,
}

impl Scheduler {
    /// Create a new scheduler.
    pub fn new(state: Arc<AppState>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let db = state.db.clone();

        let db_checkpoint = db.clone();
        let mut shutdown_rx_checkpoint = shutdown_rx.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::select! {
                    _ = sleep(Duration::from_secs(3600)) => {
                        if let Err(e) = db_checkpoint.wal_checkpoint() {
                            tracing::warn!("Periodic WAL checkpoint failed: {e}");
                        }
                        if let Err(e) = db_checkpoint.optimize() {
                            tracing::warn!("Periodic PRAGMA optimize failed: {e}");
                        }
                    },
                    _ = shutdown_rx_checkpoint.changed() => {
                        let _ = db_checkpoint.wal_checkpoint();
                        return;
                    }
                }
            }
        });

        Self {
            state,
            shutdown_tx,
            shutdown_rx,
        }
    }

    /// Start periodic tasks and return a shutdown handle.
    pub fn start(&self) -> ShutdownHandle {
        let state = self.state.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tauri::async_runtime::spawn(async move {
            tokio::select! {
                _ = sleep(Duration::from_secs(10)) => {
                    Self::run_hygiene_cleanup("startup");
                    if let Err(e) = Self::run_garbage_collection(&state).await {
                        tracing::warn!("Startup GC failed: {e}");
                    }
                },
                _ = shutdown_rx.changed() => {
                    tracing::info!("Scheduler shutting down (startup)");
                    return;
                }
            }

            loop {
                let now = Utc::now();
                let next_run = now.date_naive().and_hms_opt(3, 0, 0).unwrap();
                let next_run = if now.time() > next_run.time() {
                    next_run + chrono::Duration::days(1)
                } else {
                    next_run
                };
                let next_run = next_run.and_utc();

                let duration = (next_run - now)
                    .to_std()
                    .unwrap_or(Duration::from_secs(3600));

                tokio::select! {
                    _ = sleep(duration) => {},
                    _ = shutdown_rx.changed() => {
                        tracing::info!("Scheduler shutting down");
                        return;
                    }
                }

                Self::run_hygiene_cleanup("scheduled");
                if let Err(e) = Self::run_garbage_collection(&state).await {
                    tracing::error!("Garbage collection failed: {e}");
                }
            }
        });

        ShutdownHandle {
            tx: self.shutdown_tx.clone(),
        }
    }

    fn run_hygiene_cleanup(label: &str) {
        match crate::hygiene::cleanup_from_environment() {
            Ok(report) if report.deleted_files > 0 => tracing::info!(
                "Iris hygiene cleanup ({label}) removed {} files and freed {} bytes",
                report.deleted_files,
                report.deleted_bytes
            ),
            Ok(_) => {}
            Err(e) => tracing::warn!("Iris hygiene cleanup ({label}) failed: {e}"),
        }
    }

    async fn run_garbage_collection(state: &Arc<AppState>) -> AppResult<()> {
        let gc = GarbageCollector::new(state.cas_store()?.clone(), state.db.clone());
        let result = gc.collect().await?;

        tracing::info!(
            "Garbage collection completed: {} orphaned objects deleted, {} recycle items purged, {} bytes freed",
            result.deleted_count,
            result.recycle_purged_count,
            result.space_freed
        );

        let purged_sessions = SessionManager::purge_expired(&state.db, 90).unwrap_or(0);
        let purged_traces = SessionManager::purge_expired_traces(&state.db, 30).unwrap_or(0);
        if purged_sessions > 0 || purged_traces > 0 {
            tracing::info!(
                "Session/trace expiry: {} sessions, {} traces cleaned",
                purged_sessions,
                purged_traces
            );
        }

        Ok(())
    }
}
