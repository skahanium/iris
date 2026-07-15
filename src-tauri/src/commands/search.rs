use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::app::AppState;
use crate::embedding::engine::{semantic_search, SemanticHit};
use crate::embedding::scheduler::{EmbeddingIndexStatus, EmbeddingStartResult};
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize)]
pub struct KeywordHit {
    pub path: String,
    pub title: String,
    pub snippet: String,
}

#[tauri::command]
pub fn search_keyword(
    state: State<'_, Arc<AppState>>,
    query: String,
    limit: Option<u32>,
) -> AppResult<Vec<KeywordHit>> {
    let limit = limit.unwrap_or(20) as usize;
    state.db.with_read_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT path, title, snippet(files_fts, 2, '<b>', '</b>', '…', 32) as snip
             FROM files_fts
             WHERE files_fts MATCH ?1
               AND path <> '.classified'
               AND path NOT LIKE '.classified/%'
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![query, limit as i64], |row| {
            Ok(KeywordHit {
                path: row.get(0)?,
                title: row.get(1)?,
                snippet: row.get(2)?,
            })
        })?;
        Ok(rows.flatten().collect())
    })
}

#[tauri::command]
pub fn search_semantic(
    state: State<'_, Arc<AppState>>,
    query: String,
    limit: Option<u32>,
) -> AppResult<Vec<SemanticHit>> {
    let limit = limit.unwrap_or(5) as usize;
    state
        .db
        .with_read_conn(|conn| semantic_search(conn, &query, limit))
}

#[tauri::command]
pub fn embedding_scheduler_status(
    state: State<'_, Arc<AppState>>,
) -> AppResult<EmbeddingIndexStatus> {
    state.embedding_scheduler().status()
}

#[tauri::command]
pub fn embedding_scheduler_start(
    state: State<'_, Arc<AppState>>,
) -> AppResult<EmbeddingStartResult> {
    state
        .embedding_scheduler()
        .start_generation(crate::embedding::scheduler::EmbeddingStartSource::Manual)
}

#[tauri::command]
pub fn embedding_scheduler_set_paused(
    state: State<'_, Arc<AppState>>,
    paused: bool,
) -> AppResult<()> {
    state.embedding_scheduler().set_manual_paused(paused)
}

#[tauri::command]
pub fn embedding_scheduler_set_foreground_busy(
    state: State<'_, Arc<AppState>>,
    busy: bool,
) -> AppResult<()> {
    state.embedding_scheduler().set_foreground_busy(busy);
    Ok(())
}
