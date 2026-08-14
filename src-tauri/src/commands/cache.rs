use std::sync::Arc;

use tauri::State;

use crate::app::AppState;
use crate::cache::{
    CacheClearRequest, CacheClearResult, CacheCoordinator, CacheSummary, RuntimeCacheRepairRequest,
    RuntimeRepairResult,
};
use crate::error::AppResult;

#[tauri::command]
pub fn cache_summary(state: State<'_, Arc<AppState>>) -> AppResult<CacheSummary> {
    CacheCoordinator::new(state.paths(), &state.db).summary()
}

#[tauri::command]
pub fn cache_clear(
    state: State<'_, Arc<AppState>>,
    input: CacheClearRequest,
) -> AppResult<CacheClearResult> {
    CacheCoordinator::new(state.paths(), &state.db).clear(input)
}

#[tauri::command]
pub fn runtime_cache_repair_prepare(
    state: State<'_, Arc<AppState>>,
    input: RuntimeCacheRepairRequest,
) -> AppResult<RuntimeRepairResult> {
    CacheCoordinator::new(state.paths(), &state.db).prepare_runtime_repair(input)
}
