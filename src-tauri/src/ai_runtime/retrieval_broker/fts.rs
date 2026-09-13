use rusqlite::Connection;

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
    // A file-level hit only says the file is relevant; the citable chunk has to
    // be chosen on evidence. Preferring "contains the whole query, else the first
    // chunk" returned introductions while the answering section sat further down.
    // Rank every chunk of the file by how many query terms it covers instead.
    let tokens = query_tokens(query);
    let mut stmt = conn.prepare(
        "SELECT c.content, c.heading_path, c.source_start, c.source_end, c.content_hash
         FROM chunks AS c
         INNER JOIN files AS f ON f.id = c.file_id
         WHERE f.path = ?1
         ORDER BY c.chunk_index ASC",
    )?;
    let candidates = stmt
        .query_map(rusqlite::params![path], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let row = candidates
        .into_iter()
        .map(|candidate| (coverage(&candidate.0, &tokens), candidate))
        .max_by_key(|(hits, _)| *hits)
        .map(|(_, candidate)| candidate);
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
        .map(fts5_token)
        .filter(|token| !token.is_empty())
        .collect();
    tokens.join(" ")
}

/// One whitespace-delimited query token as an FTS5 expression.
///
/// The index stores CJK runs as overlapping bigrams plus single characters
/// (`indexer::fts::cjk_bigrams`). A query token used to be quoted verbatim, so
/// a token like `劳动合同` was looked up as one four-character term that the
/// bigram index can never contain — the query side has to expand exactly like
/// the index side. A run of two or more CJK characters therefore becomes a
/// phrase of its bigrams; anything else stays a quoted term.
fn fts5_token(token: &str) -> String {
    let cleaned: String = token
        .chars()
        .filter(|c| !c.is_control() && *c != '"')
        .collect();
    if cleaned.is_empty() {
        return String::new();
    }
    let chars: Vec<char> = cleaned.chars().collect();
    if chars.len() >= 2 && chars.iter().all(|c| crate::indexer::fts::is_cjk(*c)) {
        let bigrams: Vec<String> = chars
            .windows(2)
            .map(|window| window.iter().collect())
            .collect();
        return format!("\"{}\"", bigrams.join(" "));
    }
    format!("\"{}\"", cleaned)
}

pub(super) fn search_fts(
    conn: &Connection,
    query: &str,
    limit: usize,
    scope: &crate::ai_runtime::retrieval_scope::RetrievalScope,
) -> AppResult<Vec<ContextPacket>> {
    let safe_query = escape_fts5_query(query);
    if safe_query.is_empty() {
        return Ok(vec![]);
    }
    // `LIMIT` without an ordering made the surviving files arbitrary, so a
    // relevant note could be dropped before scope filtering ever saw it. FTS5's
    // `bm25()` is the layer's own relevance signal: lower (more negative) is a
    // better match.
    let (scope_sql, scope_values) = path_scope_predicate("f", scope);
    let mut stmt = conn.prepare(&format!(
        "SELECT f.path, f.title, bm25(files_fts) AS rank
         FROM files_fts
         JOIN files f ON f.path = files_fts.path
         WHERE files_fts MATCH ?
           AND f.path NOT LIKE '.classified/%'{scope_sql}
         ORDER BY rank ASC
         LIMIT ?"
    ))?;

    let mut bindings: Vec<rusqlite::types::Value> = vec![rusqlite::types::Value::Text(safe_query)];
    bindings.extend(scope_values);
    bindings.push(rusqlite::types::Value::Integer(limit as i64));
    let rows = stmt.query_map(rusqlite::params_from_iter(bindings), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?,
        ))
    })?;

    let mut packets = Vec::new();
    for (index, row) in rows.flatten().enumerate() {
        let (path, title, rank) = row;
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
            score: bm25_to_score(rank),
            trust_level: TrustLevel::UserNote,
            citation_label: format!("[F{index}]"),
            stale: false,
            web: None,
            corpus: None,
        });
    }
    Ok(packets)
}

/// Query terms used to score how much of a question a chunk covers.
///
/// Mirrors the index: CJK runs yield overlapping bigrams, whitespace-delimited
/// words stay whole. Single-character CJK tokens are kept because the index also
/// stores unigrams.
fn query_tokens(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for token in query.split_whitespace() {
        let chars: Vec<char> = token.chars().collect();
        if chars.len() >= 2 && chars.iter().all(|c| crate::indexer::fts::is_cjk(*c)) {
            for window in chars.windows(2) {
                let bigram: String = window.iter().collect();
                if !tokens.contains(&bigram) {
                    tokens.push(bigram);
                }
            }
        } else if !token.is_empty() && !tokens.contains(&token.to_string()) {
            tokens.push(token.to_string());
        }
    }
    tokens
}

/// How many distinct query terms appear in `content`, case-insensitively.
fn coverage(content: &str, tokens: &[String]) -> usize {
    let haystack = content.to_lowercase();
    tokens
        .iter()
        .filter(|token| haystack.contains(&token.to_lowercase()))
        .count()
}

/// Project FTS5's `bm25()` into the packet score.
///
/// `bm25()` is unbounded and negative-is-better; this is a monotone, bounded
/// projection (best match approaches 1.0, no match reaches 0.0), **not** a
/// calibrated probability. Fusion re-ranks by RRF, so the ordering it produces
/// matters more than the absolute value.
fn bm25_to_score(rank: f64) -> f64 {
    let badness = (-rank).max(0.0);
    1.0 / (1.0 + badness)
}

/// SQL predicate + bindings that constrain a query to the request's path scope.
///
/// The layers truncate their candidate pool with `LIMIT` **before** the broker
/// filters by scope, so a file inside the requested folder could be pushed out
/// by unrelated global candidates. Pushing the path part of the scope into the
/// query itself removes that recall loss; it is a recall fix, never an
/// authorisation change — `filter_packets_by_scope` still runs afterwards.
pub(super) fn path_scope_predicate(
    alias: &str,
    scope: &crate::ai_runtime::retrieval_scope::RetrievalScope,
) -> (String, Vec<rusqlite::types::Value>) {
    if scope.is_path_unrestricted() {
        return (String::new(), Vec::new());
    }
    let mut parts = Vec::new();
    let mut values: Vec<rusqlite::types::Value> = Vec::new();
    if !scope.paths.is_empty() {
        let placeholders = vec!["?"; scope.paths.len()].join(", ");
        parts.push(format!("{alias}.path IN ({placeholders})"));
        values.extend(
            scope
                .paths
                .iter()
                .cloned()
                .map(rusqlite::types::Value::Text),
        );
    }
    for prefix in &scope.path_prefixes {
        parts.push(format!("{alias}.path LIKE ?"));
        values.push(rusqlite::types::Value::Text(format!("{prefix}%")));
    }
    (format!(" AND ({})", parts.join(" OR ")), values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn cjk_query_tokens_expand_like_the_index_does() {
        // A four-character term becomes the phrase its bigrams form.
        assert_eq!(escape_fts5_query("劳动合同"), "\"劳动 动合 合同\"");
        // A two-character term is one bigram.
        assert_eq!(escape_fts5_query("劳动"), "\"劳动\"");
        // A single character is indexed as a unigram.
        assert_eq!(escape_fts5_query("劳"), "\"劳\"");
        // Non-CJK tokens keep the previous behaviour, and tokens still AND.
        assert_eq!(escape_fts5_query("risk policy"), "\"risk\" \"policy\"");
        // Quotes and control characters are still stripped.
        assert_eq!(escape_fts5_query("\"劳动\""), "\"劳动\"");
    }

    /// The reported defect, end to end at the FTS layer: index a Chinese note
    /// the way the indexer does, then search it the way the broker does.
    #[test]
    fn chinese_notes_are_found_by_multi_character_queries() {
        let conn = Connection::open_in_memory().expect("in-memory database");
        conn.execute_batch(
            "CREATE TABLE files (id INTEGER PRIMARY KEY, path TEXT, title TEXT);
             CREATE VIRTUAL TABLE files_fts USING fts5(path, title, content, tokenize='unicode61');
             CREATE TABLE chunks (
                 id INTEGER PRIMARY KEY,
                 file_id INTEGER,
                 chunk_index INTEGER,
                 heading_path TEXT,
                 content TEXT,
                 char_count INTEGER,
                 source_start INTEGER,
                 source_end INTEGER,
                 content_hash TEXT
             );",
        )
        .expect("schema");
        let body = "中华人民共和国劳动合同法";
        conn.execute(
            "INSERT INTO files (id, path, title) VALUES (1, 'notes/law.md', '劳动合同法')",
            [],
        )
        .expect("file row");
        conn.execute(
            "INSERT INTO chunks
             (file_id, chunk_index, content, char_count, source_start, source_end, content_hash)
             VALUES (1, 0, ?1, ?2, 0, ?3, 'chunk-hash')",
            rusqlite::params![
                "中华人民共和国劳动合同法",
                "中华人民共和国劳动合同法".chars().count() as i64,
                "中华人民共和国劳动合同法".len() as i64,
            ],
        )
        .expect("chunk row");
        conn.execute(
            "INSERT INTO files_fts (path, title, content) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "notes/law.md",
                "劳动合同法",
                crate::indexer::fts::cjk_bigrams(body)
            ],
        )
        .expect("indexed row");

        let hits = search_fts(&conn, "劳动", 5, &unrestricted_scope()).expect("search");
        assert_eq!(hits.len(), 1, "single bigram query must hit");
        let hits = search_fts(&conn, "劳动合同", 5, &unrestricted_scope()).expect("search");
        assert_eq!(hits.len(), 1, "four-character query must hit");
        let hits = search_fts(&conn, "合同", 5, &unrestricted_scope()).expect("search");
        assert_eq!(hits.len(), 1, "trailing bigram must hit");
    }
    /// The reported defect: the answering section must win over the first chunk.
    #[test]
    fn the_chunk_that_covers_the_query_wins_over_the_first_chunk() {
        let conn = Connection::open_in_memory().expect("in-memory database");
        conn.execute_batch(
            "CREATE TABLE files (id INTEGER PRIMARY KEY, path TEXT, title TEXT);
             CREATE VIRTUAL TABLE files_fts USING fts5(path, title, content, tokenize='unicode61');
             CREATE TABLE chunks (
                 id INTEGER PRIMARY KEY,
                 file_id INTEGER,
                 chunk_index INTEGER,
                 heading_path TEXT,
                 content TEXT,
                 char_count INTEGER,
                 source_start INTEGER,
                 source_end INTEGER,
                 content_hash TEXT
             );",
        )
        .expect("schema");
        conn.execute(
            "INSERT INTO files (id, path, title) VALUES (1, 'docs/policy.md', 'risk policy')",
            [],
        )
        .expect("file row");
        conn.execute(
            "INSERT INTO files_fts (path, title, content) VALUES ('docs/policy.md', 'risk policy', 'risk policy handbook')",
            [],
        )
        .expect("fts row");
        let introduction = "Introduction without answer.";
        let answering = "The risk policy requires a documented review.";
        for (index, body) in [(0, introduction), (1, answering)].iter() {
            conn.execute(
                "INSERT INTO chunks
                 (file_id, chunk_index, heading_path, content, char_count, source_start, source_end, content_hash)
                 VALUES (1, ?1, NULL, ?2, ?3, 0, ?4, ?5)",
                rusqlite::params![
                    index,
                    body,
                    body.chars().count() as i64,
                    body.len() as i64,
                    format!("hash-{index}")
                ],
            )
            .expect("chunk row");
        }

        let packet = chunk_evidence_for_path(&conn, "docs/policy.md", "risk policy")
            .expect("query")
            .expect("a chunk");
        assert_eq!(
            packet.content, answering,
            "the answering chunk must be chosen"
        );
    }

    /// A layer that reports relevance must order by it.
    #[test]
    fn fts_results_are_ordered_by_relevance_and_scored_monotonically() {
        assert!(bm25_to_score(-2.0) < bm25_to_score(-0.5));
        assert!(bm25_to_score(0.0) <= 1.0);
        assert!(bm25_to_score(-100.0) > 0.0);
    }

    fn unrestricted_scope() -> crate::ai_runtime::retrieval_scope::RetrievalScope {
        crate::ai_runtime::retrieval_scope::RetrievalScope {
            path_prefixes: Vec::new(),
            paths: Vec::new(),
            required_tags: Vec::new(),
        }
    }

    /// ⑦: a folder-scoped search must not lose its own file to unrelated global
    /// candidates that fill the limited pool first.
    #[test]
    fn path_scope_is_applied_before_the_candidate_limit() {
        let conn = Connection::open_in_memory().expect("in-memory database");
        conn.execute_batch(
            "CREATE TABLE files (id INTEGER PRIMARY KEY, path TEXT, title TEXT);
             CREATE VIRTUAL TABLE files_fts USING fts5(path, title, content, tokenize='unicode61');
             CREATE TABLE chunks (
                 id INTEGER PRIMARY KEY, file_id INTEGER, chunk_index INTEGER, heading_path TEXT,
                 content TEXT, char_count INTEGER, source_start INTEGER, source_end INTEGER, content_hash TEXT
             );",
        )
        .expect("schema");
        // Twenty unrelated notes that all match the query, then the scoped one.
        for index in 0..20 {
            let path = format!("outside/note-{index}.md");
            conn.execute(
                "INSERT INTO files (id, path, title) VALUES (?1, ?2, 'risk policy')",
                rusqlite::params![index + 1, path],
            )
            .expect("file row");
            conn.execute(
                "INSERT INTO files_fts (path, title, content) VALUES (?1, 'risk policy', 'risk policy body')",
                rusqlite::params![path],
            )
            .expect("fts row");
        }
        conn.execute(
            "INSERT INTO files (id, path, title) VALUES (99, 'inside/target.md', 'risk policy')",
            [],
        )
        .expect("scoped file");
        conn.execute(
            "INSERT INTO files_fts (path, title, content) VALUES ('inside/target.md', 'risk policy', 'risk policy body')",
            [],
        )
        .expect("scoped fts row");
        conn.execute(
            "INSERT INTO chunks
             (file_id, chunk_index, heading_path, content, char_count, source_start, source_end, content_hash)
             VALUES (99, 0, NULL, 'risk policy body', 15, 0, 15, 'hash')",
            [],
        )
        .expect("chunk row");

        let scoped = crate::ai_runtime::retrieval_scope::RetrievalScope {
            path_prefixes: vec!["inside/".into()],
            paths: Vec::new(),
            required_tags: Vec::new(),
        };
        // The pool is smaller than the number of global matches.
        let hits = search_fts(&conn, "risk policy", 4, &scoped).expect("scoped search");
        assert_eq!(hits.len(), 1, "the scoped file must survive the limit");
        assert_eq!(hits[0].source_path.as_deref(), Some("inside/target.md"));
    }
}
