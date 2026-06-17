use rusqlite::Connection;

use crate::ai_runtime::{ContextPacket, SourceType, TrustLevel};
use crate::error::AppResult;

fn escape_fts5_query(query: &str) -> String {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|token| {
            let cleaned: String = token
                .chars()
                .filter(|c| {
                    c.is_alphanumeric() || *c == '_' || *c == '-' || c.is_ascii_punctuation()
                })
                .collect();
            if cleaned.is_empty() {
                String::new()
            } else {
                format!("\"{}\"", cleaned.replace('"', "\"\""))
            }
        })
        .filter(|s| !s.is_empty())
        .collect();
    tokens.join(" ")
}

pub(super) fn search_fts(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> AppResult<Vec<ContextPacket>> {
    let safe_query = escape_fts5_query(query);
    if safe_query.is_empty() {
        return Ok(vec![]);
    }
    let mut stmt = conn.prepare(
        "SELECT f.path, f.title, snippet(files_fts, 2, '<b>', '</b>', '…', 40) as snippet
         FROM files_fts
         JOIN files f ON f.path = files_fts.path
         WHERE files_fts MATCH ?1
           AND f.path NOT LIKE '.classified/%'
         LIMIT ?2",
    )?;

    let rows = stmt.query_map(rusqlite::params![safe_query, limit as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;

    let packets: Vec<_> = rows
        .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!("FTS row parse failed: {e}");
                None
            }
        })
        .enumerate()
        .map(|(i, (path, title, snippet))| {
            let clean_snippet = snippet.replace("<b>", "").replace("</b>", "");
            ContextPacket {
                id: format!("fts-{i}"),
                source_type: SourceType::Note,
                source_path: Some(path),
                title,
                heading_path: None,
                source_span: None,
                content_hash: String::new(),
                excerpt: clean_snippet,
                retrieval_reason: "fts_keyword_match".into(),
                score: 0.7,
                trust_level: TrustLevel::UserNote,
                citation_label: format!("[F{i}]"),
                stale: false,
                web: None,
                corpus: None,
            }
        })
        .collect();

    Ok(packets)
}
