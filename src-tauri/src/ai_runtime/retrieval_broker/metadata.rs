use rusqlite::Connection;

use crate::ai_runtime::{ContextPacket, SourceType, TrustLevel};
use crate::error::AppResult;

use super::fts_impl::{chunk_evidence_for_path, escape_fts5_query};

/// Search aliases and tags from the dedicated metadata FTS table.
///
/// Metadata is a discovery signal only; emitted packets always quote a stored
/// body chunk so citation span and content hash remain verifiable.
pub(super) fn search_metadata(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> AppResult<Vec<ContextPacket>> {
    let safe_query = escape_fts5_query(query);
    if safe_query.is_empty() {
        return Ok(Vec::new());
    }
    let mut statement = conn.prepare(
        "SELECT f.path, f.title
         FROM files_metadata_fts AS m
         INNER JOIN files AS f ON f.path = m.path
         WHERE files_metadata_fts MATCH ?1
           AND f.path <> '.classified'
           AND f.path NOT LIKE '.classified/%'
         LIMIT ?2",
    )?;
    let rows = statement.query_map(rusqlite::params![safe_query, limit as i64], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut packets = Vec::new();
    for (index, row) in rows.flatten().enumerate() {
        let (path, title) = row;
        let Some(evidence) = chunk_evidence_for_path(conn, &path, query)? else {
            continue;
        };
        packets.push(ContextPacket {
            id: format!("metadata-{index}-{path}"),
            source_type: SourceType::Note,
            source_path: Some(path),
            title,
            heading_path: evidence.heading_path,
            source_span: Some(evidence.source_span),
            content_hash: evidence.content_hash,
            excerpt: evidence.content,
            retrieval_reason: "metadata_alias_or_tag_match".to_string(),
            score: 0.60,
            trust_level: TrustLevel::UserNote,
            citation_label: format!("[M{index}]"),
            stale: false,
            web: None,
            corpus: None,
        });
    }
    Ok(packets)
}
