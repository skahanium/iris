use std::sync::Arc;

use chrono::Utc;
use tokio::sync::watch;
use tokio::time::{sleep, Duration};

use crate::ai_runtime::session::SessionManager;
use crate::app::AppState;
use crate::cas::garbage_collector::GarbageCollector;
use crate::error::AppResult;

/// 定时任务调度器
pub struct Scheduler {
    state: Arc<AppState>,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

impl Scheduler {
    /// 创建新的调度器
    pub fn new(state: Arc<AppState>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            state,
            shutdown_tx,
            shutdown_rx,
        }
    }

    /// 启动定时任务，返回一个可用于请求关闭的 handle
    pub fn start(&self) -> ShutdownHandle {
        let state = self.state.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tauri::async_runtime::spawn(async move {
            // Run GC once shortly after startup (10s delay to avoid blocking init)
            tokio::select! {
                _ = sleep(Duration::from_secs(10)) => {
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

                if let Err(e) = Self::run_garbage_collection(&state).await {
                    tracing::error!("Garbage collection failed: {e}");
                }
            }
        });

        ShutdownHandle {
            tx: self.shutdown_tx.clone(),
        }
    }

    /// 执行垃圾回收
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

/// 用于请求调度器关闭的 handle
///
/// 当前由 `lib.rs` 持有，应用退出时自动 drop。
/// 若需主动取消（如测试场景），可调用 [`ShutdownHandle::shutdown`]。
#[allow(dead_code)]
pub struct ShutdownHandle {
    tx: watch::Sender<bool>,
}

#[allow(dead_code)]
impl ShutdownHandle {
    /// 请求调度器停止
    pub fn shutdown(&self) {
        let _ = self.tx.send(true);
    }
}
