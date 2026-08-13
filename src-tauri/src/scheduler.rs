use std::sync::Arc;

use chrono::Utc;
use rusqlite::OptionalExtension;
use tokio::sync::watch;
use tokio::time::{sleep, Duration};

use crate::ai_runtime::normal_session_repository::NormalSessionRepository;
use crate::app::AppState;
use crate::cas::garbage_collector::GarbageCollector;
use crate::error::AppResult;

/// Periodic background task scheduler.
pub struct Scheduler {
    state: Arc<AppState>,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

/// Handle returned by [`Scheduler::start`]. Currently a lifetime token only:
/// spawned tasks listen on the scheduler's own receiver; dropping this handle
/// does not stop them.
pub struct ShutdownHandle {
    _tx: watch::Sender<bool>,
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

        // 订阅自动同步：启动 30 秒后先跑一轮（按 next_fetch_at 恢复重启前
        // 的到期源），之后每 15 分钟取最多 2 个到期源并发同步。
        let feed_sync = state.feed_sync.clone();
        let feed_fulltext = state.feed_fulltext.clone();
        let feed_db = state.db.clone();
        let feed_wake = state.feed_sync_wake.clone();
        let mut shutdown_rx_feed = shutdown_rx.clone();
        tauri::async_runtime::spawn(async move {
            tokio::select! {
                _ = sleep(Duration::from_secs(30)) => {},
                _ = shutdown_rx_feed.changed() => return,
            }
            loop {
                let enabled = feed_db
                    .with_read_conn(|conn| {
                        let raw: Option<String> = conn
                        .query_row(
                            "SELECT value FROM settings WHERE key = 'feed_background_sync_enabled'",
                            [],
                            |row| row.get(0),
                        )
                        .optional()?;
                        Ok(raw
                            .and_then(|value| serde_json::from_str::<bool>(&value).ok())
                            .unwrap_or(true))
                    })
                    .unwrap_or(true);
                if enabled {
                    if let Err(error) = feed_sync.sync_due_batch().await {
                        tracing::warn!(error_code = %error, "feed_sync_all failed");
                    }
                    feed_fulltext.schedule();
                }
                tokio::select! {
                    _ = sleep(Duration::from_secs(15 * 60)) => {},
                    _ = feed_wake.notified() => {},
                    _ = shutdown_rx_feed.changed() => return,
                }
            }
        });

        // RSS 保留维护与同步分离：过期条目先进入 RSS 专属回收站，30 天后
        // 才物理清理；收藏不参与。绝不在后台 VACUUM，以免锁住正在阅读的库。
        let feed_retention_db = state.db.clone();
        let mut shutdown_rx_retention = shutdown_rx.clone();
        tauri::async_runtime::spawn(async move {
            tokio::select! {
                _ = sleep(Duration::from_secs(45)) => {},
                _ = shutdown_rx_retention.changed() => return,
            }
            loop {
                let result = feed_retention_db.with_conn(|conn| {
                    let soft_deleted =
                        crate::feed::repository::FeedRepository::soft_delete_expired_items(
                            conn,
                            Utc::now(),
                        )?;
                    let purged = crate::feed::repository::FeedRepository::purge_deleted_items(
                        conn,
                        Utc::now(),
                    )?;
                    Ok((soft_deleted, purged))
                });
                if let Ok((soft_deleted, purged)) = result {
                    if soft_deleted > 0 || purged > 0 {
                        tracing::info!(soft_deleted, purged, "feed_retention_maintenance_complete");
                    }
                } else {
                    tracing::warn!(
                        result_code = "feed_retention_maintenance_failed",
                        "feed_retention_maintenance_failed"
                    );
                }
                tokio::select! {
                    _ = sleep(Duration::from_secs(24 * 60 * 60)) => {},
                    _ = shutdown_rx_retention.changed() => return,
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
            _tx: self.shutdown_tx.clone(),
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

        let purged_sessions = NormalSessionRepository::purge_expired(&state.db, 90).unwrap_or(0);
        if purged_sessions > 0 {
            tracing::info!(purged_sessions, "Expired Run sessions cleaned");
        }

        Ok(())
    }
}
