#![allow(dead_code)]

use std::sync::Arc;

use chrono::Utc;
use tokio::time::{sleep, Duration};

use crate::app::AppState;
use crate::cas::garbage_collector::GarbageCollector;
use crate::error::AppResult;

/// 定时任务调度器
pub struct Scheduler {
    state: Arc<AppState>,
}

impl Scheduler {
    /// 创建新的调度器
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// 启动定时任务
    pub fn start(&self) {
        let state = self.state.clone();

        // 每天凌晨 3:00 执行垃圾回收
        tokio::spawn(async move {
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
                sleep(duration).await;

                if let Err(e) = Self::run_garbage_collection(&state).await {
                    tracing::error!("Garbage collection failed: {e}");
                }
            }
        });
    }

    /// 执行垃圾回收
    async fn run_garbage_collection(state: &Arc<AppState>) -> AppResult<()> {
        let gc = GarbageCollector::new(state.cas_store().clone(), state.db.clone());
        let result = gc.collect()?;

        tracing::info!(
            "Garbage collection completed: {} orphaned objects deleted, {} recycle items purged, {} bytes freed",
            result.deleted_count,
            result.recycle_purged_count,
            result.space_freed
        );

        Ok(())
    }
}
