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
    scope: &crate::ai_runtime::retrieval_scope::RetrievalScope,
) -> AppResult<Vec<ContextPacket>> {
    let safe_query = escape_fts5_query(query);
    if safe_query.is_empty() {
        return Ok(Vec::new());
    }
    let (scope_sql, scope_values) = super::fts_impl::path_scope_predicate("f", scope);
    // Metadata carries no relevance signal of its own (every packet scores the
    // same), so its candidates are ordered by path. Without an `ORDER BY`,
    // `LIMIT` returned insertion order, which let unrelated notes fill the
    // candidate pool before the scoped ones were ever considered.
    let mut statement = conn.prepare(&format!(
        "SELECT f.path, f.title
         FROM files_metadata_fts AS m
         INNER JOIN files AS f ON f.path = m.path
         WHERE files_metadata_fts MATCH ?
           AND f.path <> '.classified'
           AND f.path NOT LIKE '.classified/%'{scope_sql}
         ORDER BY f.path ASC
         LIMIT ?"
    ))?;
    let mut bindings: Vec<rusqlite::types::Value> = vec![rusqlite::types::Value::Text(safe_query)];
    bindings.extend(scope_values);
    bindings.push(rusqlite::types::Value::Integer(limit as i64));
    let rows = statement.query_map(rusqlite::params_from_iter(bindings), |row| {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::retrieval_scope::RetrievalScope;

    fn fixture() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory database");
        conn.execute_batch(
            "CREATE TABLE files (id INTEGER PRIMARY KEY, path TEXT, title TEXT);
             CREATE VIRTUAL TABLE files_metadata_fts USING fts5(path, aliases, tags, tokenize='unicode61');
             CREATE TABLE chunks (
                 id INTEGER PRIMARY KEY, file_id INTEGER, chunk_index INTEGER, heading_path TEXT,
                 content TEXT, char_count INTEGER, source_start INTEGER, source_end INTEGER, content_hash TEXT
             );",
        )
        .expect("schema");
        conn
    }

    fn insert(conn: &Connection, id: i64, path: &str, alias: &str) {
        conn.execute(
            "INSERT INTO files (id, path, title) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, path, alias],
        )
        .expect("file row");
        conn.execute(
            "INSERT INTO files_metadata_fts (path, aliases, tags) VALUES (?1, ?2, '')",
            rusqlite::params![path, alias],
        )
        .expect("metadata row");
        conn.execute(
            "INSERT INTO chunks
             (file_id, chunk_index, heading_path, content, char_count, source_start, source_end, content_hash)
             VALUES (?1, 0, NULL, ?2, 5, 0, 5, ?3)",
            rusqlite::params![id, alias, format!("hash-{id}")],
        )
        .expect("chunk row");
    }

    /// A scoped metadata request must be served by the scoped note even when
    /// unrelated notes could fill the candidate pool first.
    ///
    /// The scoped query is the discriminating half: without the pushed-down
    /// path predicate, every candidate row comes from `outside/`, so the layer
    /// returns nothing after the broker's own scope filter.
    #[test]
    fn scoped_metadata_candidates_cannot_be_filled_by_unrelated_notes() {
        let conn = fixture();
        for index in 0..20 {
            insert(
                &conn,
                index + 1,
                &format!("outside/note-{index}.md"),
                "phoenix",
            );
        }
        insert(&conn, 99, "inside/target.md", "phoenix");

        let scoped = RetrievalScope {
            path_prefixes: vec!["inside/".into()],
            paths: Vec::new(),
            required_tags: Vec::new(),
        };
        let hits = search_metadata(&conn, "phoenix", 4, &scoped).expect("scoped metadata search");

        assert_eq!(
            hits.iter()
                .map(|packet| packet.source_path.clone())
                .collect::<Vec<_>>(),
            vec![Some("inside/target.md".to_string())],
            "the scoped note must be the one the metadata layer returns"
        );
    }

    /// An unscoped request must draw from every matching note, not only from
    /// the `LIMIT`-sized head of an arbitrary scan order.
    ///
    /// The metadata layer had no `ORDER BY`, so a candidate pool smaller than
    /// the match set could be the same few rows every time while the rest were
    /// never eligible.
    #[test]
    fn unscoped_metadata_candidates_come_from_the_whole_match_set() {
        let conn = fixture();
        for index in 0..20 {
            insert(
                &conn,
                index + 1,
                &format!("outside/note-{index}.md"),
                "phoenix",
            );
        }
        insert(&conn, 99, "inside/target.md", "phoenix");

        let pool = search_metadata(&conn, "phoenix", 4, &RetrievalScope::default())
            .expect("unscoped metadata search");
        assert_eq!(pool.len(), 4);
        assert!(
            pool.iter()
                .any(|packet| packet.source_path.as_deref() == Some("inside/target.md")),
            "a scoped follow-up must be able to reach the last matching note, got {:?}",
            pool.iter()
                .map(|packet| packet.source_path.clone())
                .collect::<Vec<_>>()
        );
    }

    /// Unscoped metadata discovery still returns global matches in a stable
    /// order rather than relying on insertion order.
    #[test]
    fn unscoped_metadata_candidates_are_ordered_deterministically() {
        let conn = fixture();
        for index in 0..3 {
            insert(
                &conn,
                index + 1,
                &format!("notes/note-{index}.md"),
                "phoenix",
            );
        }
        let first = search_metadata(&conn, "phoenix", 2, &RetrievalScope::default())
            .expect("metadata search");
        let second = search_metadata(&conn, "phoenix", 2, &RetrievalScope::default())
            .expect("metadata search");
        assert_eq!(first.len(), 2);
        let paths = |packets: &[ContextPacket]| {
            packets
                .iter()
                .map(|packet| packet.source_path.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(paths(&first), paths(&second));
    }
}
