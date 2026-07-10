use rusqlite::{Connection, OptionalExtension};

use crate::ai_runtime::{ContextPacket, SourceSpan, SourceType, TrustLevel};
use crate::error::AppResult;

#[derive(Debug, Clone)]
pub(super) struct ChunkEvidence {
    pub content: String,
    pub heading_path: Option<String>,
    pub source_span: SourceSpan,
    pub content_hash: String,
}

/// Load a citable chunk for one retrieved file. The broker never turns a
/// file-level FTS hit into a packet without a stored span and content hash.
pub(super) fn chunk_evidence_for_path(
    conn: &Connection,
    path: &str,
    query: &str,
) -> AppResult<Option<ChunkEvidence>> {
    let row = conn
        .query_row(
            "SELECT c.content, c.heading_path, c.source_start, c.source_end, c.content_hash
         FROM chunks AS c
         INNER JOIN files AS f ON f.id = c.file_id
         WHERE f.path = ?1
         ORDER BY CASE WHEN instr(lower(c.content), lower(?2)) > 0 THEN 0 ELSE 1 END,
                  c.chunk_index ASC
         LIMIT 1",
            rusqlite::params![path, query],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .optional()?;
    let Some((content, heading_path, start, end, content_hash)) = row else {
        return Ok(None);
    };
    let (Some(start), Some(end), Some(content_hash)) = (start, end, content_hash) else {
        return Ok(None);
    };
    if start < 0 || end < start || content_hash.is_empty() {
        return Ok(None);
    }
    Ok(Some(ChunkEvidence {
        content,
        heading_path,
        source_span: SourceSpan {
            start: start as usize,
            end: end as usize,
        },
        content_hash,
    }))
}

pub fn escape_fts5_query(query: &str) -> String {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|token| {
            let cleaned: String = token
                .chars()
                .filter(|c| !c.is_control() && *c != '"')
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
        "SELECT f.path, f.title
         FROM files_fts
         JOIN files f ON f.path = files_fts.path
         WHERE files_fts MATCH ?1
           AND f.path NOT LIKE '.classified/%'
         LIMIT ?2",
    )?;

    let rows = stmt.query_map(rusqlite::params![safe_query, limit as i64], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut packets = Vec::new();
    for (index, row) in rows.flatten().enumerate() {
        let (path, title) = row;
        let Some(evidence) = chunk_evidence_for_path(conn, &path, query)? else {
            continue;
        };
        packets.push(ContextPacket {
            id: format!("fts-{index}-{path}"),
            source_type: SourceType::Note,
            source_path: Some(path),
            title,
            heading_path: evidence.heading_path,
            source_span: Some(evidence.source_span),
            content_hash: evidence.content_hash,
            excerpt: evidence.content,
            retrieval_reason: "fts_keyword_match".into(),
            score: 0.7,
            trust_level: TrustLevel::UserNote,
            citation_label: format!("[F{index}]"),
            stale: false,
            web: None,
            corpus: None,
        });
    }
    Ok(packets)
}
