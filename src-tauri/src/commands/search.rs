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
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    state
        .db
        .with_read_conn(|conn| search_keyword_with_fallback(conn, &query, limit))
}

fn search_keyword_with_fallback(
    conn: &rusqlite::Connection,
    query: &str,
    limit: usize,
) -> AppResult<Vec<KeywordHit>> {
    // Prefer the raw FTS5 query so advanced syntax such as `foo OR bar`
    // keeps working, then fall back to an escaped phrase query when the
    // input would make MATCH fail (unmatched quotes, control tokens, ...).
    // This keeps the UI panel consistent with the retrieval broker's
    // keyword layer instead of surfacing a malformed MATCH error.
    match search_keyword_hits(conn, query, limit) {
        Ok(hits) => Ok(hits),
        Err(error) if is_fts_syntax_error(&error) => {
            let safe = crate::ai_runtime::retrieval_broker::escape_fts5_query(query);
            if safe.trim().is_empty() {
                return Ok(Vec::new());
            }
            search_keyword_hits(conn, &safe, limit).map_err(Into::into)
        }
        Err(error) => Err(error.into()),
    }
}

fn search_keyword_hits(
    conn: &rusqlite::Connection,
    query: &str,
    limit: usize,
) -> rusqlite::Result<Vec<KeywordHit>> {
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
    // Collect eagerly so query-time failures (e.g. a malformed MATCH
    // expression) propagate instead of being silently swallowed by flatten.
    rows.collect::<rusqlite::Result<Vec<KeywordHit>>>()
}

/// True when the SQLite failure is a malformed FTS5 MATCH expression that an
/// escaped fallback query can avoid.
fn is_fts_syntax_error(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(_, Some(detail))
            if detail.contains("malformed MATCH expression")
                || detail.starts_with("fts5:")
                || detail.contains("unterminated string")
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::Database;

    fn open_db_with_notes() -> Database {
        let db = Database::open_in_memory().expect("open database");
        db.with_conn(|conn| {
            for (path, title, body) in [
                ("notes/alpha.md", "Alpha", "alpha beta gamma"),
                ("notes/beta.md", "Beta", "hello world"),
                ("notes/quote.md", "Quote", "say \"quoted\" text"),
            ] {
                conn.execute(
                    "INSERT INTO files_fts (path, title, content) VALUES (?1, ?2, ?3)",
                    rusqlite::params![path, title, body],
                )
                .expect("index note");
            }
            Ok(())
        })
        .expect("seed notes");
        db
    }

    #[test]
    fn plain_query_returns_matching_keyword_hits() {
        let db = open_db_with_notes();
        db.with_read_conn(|conn| {
            // "gamma" only occurs in alpha.md's content; note that FTS5 matches
            // across all columns, so a query that appears in a title would hit
            // multiple rows.
            let hits = search_keyword_with_fallback(conn, "gamma", 20).expect("search");
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0].path, "notes/alpha.md");
            Ok(())
        })
        .expect("read");
    }

    #[test]
    fn advanced_fts_syntax_still_works_unchanged() {
        let db = open_db_with_notes();
        db.with_read_conn(|conn| {
            // `OR` is a valid FTS5 operator: the raw query path must keep
            // working without escaping so advanced syntax is preserved.
            let hits = search_keyword_with_fallback(conn, "beta OR world", 20).expect("search");
            assert_eq!(hits.len(), 2);
            Ok(())
        })
        .expect("read");
    }

    #[test]
    fn unmatched_quote_falls_back_to_escaped_query_without_error() {
        let db = open_db_with_notes();
        db.with_read_conn(|conn| {
            // An unmatched quote makes the raw MATCH expression malformed;
            // the fallback must return hits instead of surfacing the error
            // (previously the error was silently swallowed by flatten and the
            // panel showed empty results).
            let hits = search_keyword_with_fallback(conn, "\"quoted", 20).expect("search");
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0].path, "notes/quote.md");
            Ok(())
        })
        .expect("read");
    }

    #[test]
    fn punctuation_heavy_query_falls_back_gracefully() {
        let db = open_db_with_notes();
        db.with_read_conn(|conn| {
            let hits = search_keyword_with_fallback(conn, "foo (bar) -baz!", 20).expect("search");
            // Must not panic nor surface a malformed MATCH error; the escaped
            // fallback simply returns the matches it can find.
            assert!(hits.iter().all(|hit| !hit.path.is_empty()));
            Ok(())
        })
        .expect("read");
    }

    #[test]
    fn empty_query_returns_empty() {
        let db = open_db_with_notes();
        db.with_read_conn(|conn| {
            let hits = search_keyword_with_fallback(conn, "   ", 20).expect("search");
            assert!(hits.is_empty());
            Ok(())
        })
        .expect("read");
    }
}
