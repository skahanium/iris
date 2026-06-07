use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::app::AppState;
use crate::embedding::engine::{semantic_search, SemanticHit};
use crate::error::AppResult;
use crate::indexer::scan::index_vault_incremental;

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
pub fn search_reindex(state: State<'_, Arc<AppState>>) -> AppResult<usize> {
    let vault = state.vault_path()?;
    let entries = state
        .db
        .with_conn(|conn| index_vault_incremental(conn, &vault, Some(state.inner())))?;
    Ok(entries.len())
}
