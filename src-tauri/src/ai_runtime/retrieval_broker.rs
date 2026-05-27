//! Hybrid retrieval broker — unified search across five layers.
//!
//! Layers: FTS → Vector → Graph → Exact Parser → Template
//! Results are fused by weighted score and returned as ContextPackets.

use rusqlite::Connection;

use crate::ai_runtime::{ContextPacket, SourceType, TrustLevel};
use crate::embedding::engine;
use crate::error::AppResult;

// ─── Retrieval Request ───────────────────────────────────

#[derive(Debug, Clone)]
pub struct RetrievalRequest {
    pub query: String,
    pub max_results: usize,
    pub layers: RetrievalLayers,
    pub note_context: Option<String>, // current note path for graph/backlink boost
    pub file_id_context: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct RetrievalLayers {
    pub fts: bool,
    pub vector: bool,
    pub graph: bool,
    pub exact: bool,    // regulation exact match
    pub template: bool, // genre template match
}

impl Default for RetrievalLayers {
    fn default() -> Self {
        Self {
            fts: true,
            vector: true,
            graph: true,
            exact: true,
            template: false,
        }
    }
}

// ─── Unified Retrieval ───────────────────────────────────

/// Execute hybrid retrieval and return ContextPackets.
pub fn hybrid_retrieve(
    conn: &Connection,
    request: &RetrievalRequest,
) -> AppResult<Vec<ContextPacket>> {
    let mut packets: Vec<ContextPacket> = Vec::new();

    // Layer 1: FTS (keyword + regulation name)
    if request.layers.fts {
        if let Ok(fts_results) = search_fts(conn, &request.query, request.max_results) {
            packets.extend(fts_results);
        }
    }

    // Layer 2: Vector (anchors + regulations)
    if request.layers.vector {
        if let Ok(vec_results) = search_vector_anchors(conn, &request.query, request.max_results) {
            packets.extend(vec_results);
        }
        if let Ok(reg_results) =
            search_vector_regulations(conn, &request.query, request.max_results)
        {
            packets.extend(reg_results);
        }
    }

    // Layer 3: Graph (confirmed links)
    if request.layers.graph {
        if let Some(file_id) = request.file_id_context {
            if let Ok(graph_results) =
                search_graph_neighbors(conn, file_id, request.max_results / 2)
            {
                packets.extend(graph_results);
            }
        }
    }

    // Layer 4: Exact parser (regulation article lookup)
    if request.layers.exact {
        if let Ok(exact_results) = search_exact_regulation(conn, &request.query) {
            packets.extend(exact_results);
        }
    }

    // Deduplicate and sort by score
    packets.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    packets.dedup_by(|a, b| a.id == b.id);
    packets.truncate(request.max_results);

    Ok(packets)
}

// ─── Layer Implementations ───────────────────────────────

fn search_fts(conn: &Connection, query: &str, limit: usize) -> AppResult<Vec<ContextPacket>> {
    // Use existing FTS5 search
    let mut stmt = conn.prepare(
        "SELECT f.path, f.title, snippet(files_fts, 2, '<b>', '</b>', '…', 40) as snippet
         FROM files_fts
         JOIN files f ON f.path = files_fts.path
         WHERE files_fts MATCH ?1
         LIMIT ?2",
    )?;

    let rows = stmt.query_map(rusqlite::params![query, limit as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;

    let packets: Vec<_> = rows
        .flatten()
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
            }
        })
        .collect();

    Ok(packets)
}

fn search_vector_anchors(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> AppResult<Vec<ContextPacket>> {
    let query_vec = engine::embed_text(query)?;
    let blob = engine::f32_to_bytes(&query_vec);

    let mut stmt = match conn.prepare(
        "SELECT va.rowid, sa.content, f.path, f.title, sa.heading_path,
                sa.anchor_type, sa.confidence, va.distance
         FROM vec_anchors va
         JOIN semantic_anchors sa ON sa.id = va.rowid
         JOIN files f ON f.id = sa.file_id
         WHERE va.embedding MATCH ?1
         ORDER BY va.distance
         LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(_) => return Ok(vec![]), // vec_anchors table may not exist yet
    };

    let rows = stmt.query_map(rusqlite::params![blob, limit as i64], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, f64>(6)?,
            row.get::<_, f64>(7)?,
        ))
    })?;

    let packets: Vec<_> = rows
        .flatten()
        .enumerate()
        .map(
            |(i, (rowid, content, path, title, heading, anchor_type, _confidence, distance))| {
                let score = (1.0 - distance).max(0.0);
                ContextPacket {
                    id: format!("anchor-{rowid}"),
                    source_type: SourceType::Anchor,
                    source_path: Some(path),
                    title,
                    heading_path: heading,
                    source_span: None,
                    content_hash: String::new(),
                    excerpt: truncate(&content, 300),
                    retrieval_reason: format!("vector_{anchor_type}"),
                    score,
                    trust_level: TrustLevel::DerivedCache,
                    citation_label: format!("[A{i}]"),
                    stale: false,
                }
            },
        )
        .collect();

    Ok(packets)
}

fn search_vector_regulations(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> AppResult<Vec<ContextPacket>> {
    let query_vec = engine::embed_text(query)?;
    let blob = engine::f32_to_bytes(&query_vec);

    let mut stmt = match conn.prepare(
        "SELECT vr.rowid, ri.content, f.path, f.title, ri.regulation_name,
                ri.article, ri.paragraph, vr.distance
         FROM vec_regulations vr
         JOIN regulation_index ri ON ri.id = vr.rowid
         JOIN files f ON f.id = ri.file_id
         WHERE vr.embedding MATCH ?1
         ORDER BY vr.distance
         LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(_) => return Ok(vec![]),
    };

    let rows = stmt.query_map(rusqlite::params![blob, limit as i64], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, f64>(7)?,
        ))
    })?;

    let packets: Vec<_> = rows
        .flatten()
        .map(
            |(rowid, content, path, title, reg_name, article, paragraph, distance)| {
                let score = (1.0 - distance).max(0.0);
                let citation = match &paragraph {
                    Some(p) => format!("{reg_name} {article}{p}"),
                    None => format!("{reg_name} {article}"),
                };
                ContextPacket {
                    id: format!("reg-{rowid}"),
                    source_type: SourceType::Regulation,
                    source_path: Some(path),
                    title,
                    heading_path: Some(format!("{reg_name} > {article}")),
                    source_span: None,
                    content_hash: String::new(),
                    excerpt: truncate(&content, 400),
                    retrieval_reason: "vector_regulation_match".into(),
                    score,
                    trust_level: TrustLevel::DerivedCache,
                    citation_label: citation,
                    stale: false,
                }
            },
        )
        .collect();

    Ok(packets)
}

fn search_graph_neighbors(
    conn: &Connection,
    file_id: i64,
    limit: usize,
) -> AppResult<Vec<ContextPacket>> {
    let mut stmt = conn.prepare(
        "SELECT bl.id, bl.target_file_id, f.path, f.title, bl.target_anchor_key,
                bl.confidence, bl.link_type
         FROM block_links bl
         JOIN files f ON f.id = bl.target_file_id
         WHERE bl.source_file_id = ?1 AND bl.is_confirmed = 1
         ORDER BY bl.confidence DESC
         LIMIT ?2",
    )?;

    let rows = stmt.query_map(rusqlite::params![file_id, limit as i64], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, f64>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;

    let packets: Vec<_> = rows
        .flatten()
        .enumerate()
        .map(
            |(i, (id, _target_id, path, title, anchor_key, confidence, link_type))| ContextPacket {
                id: format!("link-{id}"),
                source_type: SourceType::Note,
                source_path: Some(path),
                title,
                heading_path: anchor_key,
                source_span: None,
                content_hash: String::new(),
                excerpt: format!("linked via {link_type}"),
                retrieval_reason: format!("graph_{link_type}"),
                score: confidence,
                trust_level: TrustLevel::UserNote,
                citation_label: format!("[L{i}]"),
                stale: false,
            },
        )
        .collect();

    Ok(packets)
}

fn search_exact_regulation(conn: &Connection, query: &str) -> AppResult<Vec<ContextPacket>> {
    // Try exact article lookup: regulation name + article number
    let re = regex::Regex::new(r"《([^》]+)》\s*第([一二三四五六七八九十百千0-9]+)条").unwrap();
    let Some(caps) = re.captures(query) else {
        return Ok(vec![]);
    };

    let reg_name = format!("《{}》", &caps[1]);
    let article = format!("第{}条", &caps[2]);

    let mut stmt = conn.prepare(
        "SELECT ri.id, ri.content, f.path, f.title, ri.regulation_name,
                ri.article, ri.paragraph
         FROM regulation_index ri
         JOIN files f ON f.id = ri.file_id
         WHERE ri.regulation_name = ?1 AND ri.article = ?2
         LIMIT 5",
    )?;

    let rows = stmt.query_map(rusqlite::params![reg_name, article], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;

    let packets: Vec<_> = rows
        .flatten()
        .map(|(id, content, path, title, reg_name, article, paragraph)| {
            let citation = match &paragraph {
                Some(p) => format!("{reg_name} {article}{p}"),
                None => format!("{reg_name} {article}"),
            };
            ContextPacket {
                id: format!("exact-{id}"),
                source_type: SourceType::Regulation,
                source_path: Some(path),
                title,
                heading_path: Some(format!("{reg_name} > {article}")),
                source_span: None,
                content_hash: String::new(),
                excerpt: truncate(&content, 500),
                retrieval_reason: "exact_regulation_lookup".into(),
                score: 0.99,
                trust_level: TrustLevel::UserNote,
                citation_label: citation,
                stale: false,
            }
        })
        .collect();

    Ok(packets)
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max_chars).collect::<String>())
    }
}

// ─── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrieval_request_default_layers() {
        let req = RetrievalRequest {
            query: "test".into(),
            max_results: 10,
            layers: RetrievalLayers::default(),
            note_context: None,
            file_id_context: None,
        };
        assert!(req.layers.fts);
        assert!(req.layers.vector);
        assert!(req.layers.graph);
        assert!(req.layers.exact);
        assert!(!req.layers.template);
    }

    #[test]
    fn exact_regulation_regex_matches() {
        let query = "《纪律处分条例》第六条怎么规定";
        let re = regex::Regex::new(r"《([^》]+)》\s*第([一二三四五六七八九十百千0-9]+)条").unwrap();
        assert!(re.is_match(query));
    }

    #[test]
    fn hybrid_retrieve_empty_db_returns_empty() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let req = RetrievalRequest {
            query: "测试".into(),
            max_results: 10,
            layers: RetrievalLayers::default(),
            note_context: None,
            file_id_context: None,
        };
        let packets = hybrid_retrieve(&conn, &req).unwrap();
        // No tables exist in a fresh in-memory DB, so all layers should fail gracefully
        assert!(packets.is_empty());
    }

    #[test]
    fn truncate_within_limit() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exceeds_limit() {
        let long = "a".repeat(100);
        let result = truncate(&long, 20);
        assert!(result.ends_with('…'));
        assert_eq!(result.chars().count(), 21); // 20 chars + '…'
    }

    #[test]
    fn truncate_empty() {
        assert_eq!(truncate("", 10), "");
    }
}
