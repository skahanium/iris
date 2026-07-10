use rusqlite::{Connection, OptionalExtension};

use crate::ai_runtime::{ContextPacket, SourceSpan, SourceType, TrustLevel};
use crate::error::AppResult;

/// Retrieve actual target-file chunks for confirmed graph neighbors.
///
/// A graph edge only discovers a related note; it is never evidence on its
/// own. Each emitted packet therefore contains source text and its stored span.
pub(super) fn search_graph_neighbors(
    conn: &Connection,
    file_id: i64,
    limit: usize,
) -> AppResult<Vec<ContextPacket>> {
    let mut statement = conn.prepare(
        "SELECT bl.id, bl.target_file_id, f.path, f.title, bl.confidence, bl.link_type
         FROM block_links AS bl
         INNER JOIN files AS f ON f.id = bl.target_file_id
         WHERE bl.source_file_id = ?1
           AND bl.is_confirmed = 1
           AND f.path <> '.classified'
           AND f.path NOT LIKE '.classified/%'
         ORDER BY bl.confidence DESC, bl.id ASC
         LIMIT ?2",
    )?;
    let links = statement
        .query_map(rusqlite::params![file_id, limit as i64], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut packets = Vec::new();
    for (link_id, target_file_id, path, title, confidence, link_type) in links {
        let chunk = conn
            .query_row(
                "SELECT id, content, heading_path, source_start, source_end, content_hash
                 FROM chunks
                 WHERE file_id = ?1
                 ORDER BY chunk_index ASC, id ASC
                 LIMIT 1",
                [target_file_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                        row.get::<_, Option<i64>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((chunk_id, content, heading_path, start, end, content_hash)) = chunk else {
            continue;
        };
        let source_span = match (start, end) {
            (Some(start), Some(end)) if start >= 0 && end >= start => Some(SourceSpan {
                start: start as usize,
                end: end as usize,
            }),
            _ => None,
        };
        let index = packets.len();
        packets.push(ContextPacket {
            id: format!("graph-{link_id}-chunk-{chunk_id}"),
            source_type: SourceType::Note,
            source_path: Some(path),
            title,
            heading_path,
            source_span,
            content_hash: content_hash.unwrap_or_default(),
            excerpt: truncate_chars(&content, 400),
            retrieval_reason: format!("graph_{link_type}"),
            score: confidence.clamp(0.0, 1.0),
            trust_level: TrustLevel::UserNote,
            citation_label: format!("[G{index}]"),
            stale: false,
            web: None,
            corpus: None,
        });
    }
    Ok(packets)
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::search_graph_neighbors;
    use rusqlite::Connection;

    #[test]
    fn graph_neighbors_return_target_chunk_content_as_evidence() {
        let conn = Connection::open_in_memory().expect("open database");
        conn.execute_batch(
            "CREATE TABLE files (id INTEGER PRIMARY KEY, path TEXT, title TEXT);
             CREATE TABLE chunks (
                id INTEGER PRIMARY KEY, file_id INTEGER, chunk_index INTEGER, content TEXT,
                heading_path TEXT, source_start INTEGER, source_end INTEGER, content_hash TEXT
             );
             CREATE TABLE block_links (
                id INTEGER PRIMARY KEY, source_file_id INTEGER, target_file_id INTEGER,
                target_anchor_key TEXT, confidence REAL, link_type TEXT, is_confirmed INTEGER
             );
             INSERT INTO files VALUES (1, 'source.md', 'Source'), (2, 'target.md', 'Target');
             INSERT INTO chunks VALUES (7, 2, 0, 'target evidence body', 'Heading', 4, 24, 'hash');
             INSERT INTO block_links VALUES (9, 1, 2, NULL, 0.8, 'wikilink', 1);",
        )
        .expect("seed graph");

        let packets = search_graph_neighbors(&conn, 1, 3).expect("graph retrieval");

        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].excerpt, "target evidence body");
        assert_eq!(packets[0].content_hash, "hash");
        assert!(packets[0].source_span.is_some());
        assert_eq!(packets[0].retrieval_reason, "graph_wikilink");
    }
}
