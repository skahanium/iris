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

    // Layer 2: Vector (chunks + anchors + regulations)
    if request.layers.vector {
        if let Ok(chunk_results) = search_vector_chunks(conn, &request.query, request.max_results) {
            packets.extend(chunk_results);
        }
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

    // Layer 5: Template (genre template match)
    if request.layers.template {
        if let Ok(template_results) = search_template(conn, &request.query, request.max_results) {
            packets.extend(template_results);
        }
    }

    // Score fusion: normalize and weight by layer, then deduplicate
    fuse_and_rank(&mut packets, request.max_results);

    Ok(packets)
}

// ─── Layer Implementations ───────────────────────────────

fn search_vector_chunks(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> AppResult<Vec<ContextPacket>> {
    let query_vec = engine::embed_text(query)?;
    let blob = engine::f32_to_bytes(&query_vec);

    let mut stmt = match conn.prepare(
        "SELECT vc.rowid, c.text, f.path, f.title, c.heading_path,
                c.char_count, vc.distance
         FROM vec_chunks vc
         JOIN chunks c ON c.id = vc.rowid
         JOIN files f ON f.id = c.file_id
         WHERE vc.embedding MATCH ?1
         ORDER BY vc.distance
         LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(_) => return Ok(vec![]), // vec_chunks table may not exist yet
    };

    let rows = stmt.query_map(rusqlite::params![blob, limit as i64], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, f64>(6)?,
        ))
    })?;

    let packets: Vec<_> = rows
        .flatten()
        .enumerate()
        .map(
            |(i, (rowid, text, path, title, heading, _char_count, distance))| {
                let score = (1.0 - distance).max(0.0);
                ContextPacket {
                    id: format!("chunk-{rowid}"),
                    source_type: SourceType::Note,
                    source_path: Some(path),
                    title: title.clone(),
                    heading_path: heading,
                    source_span: None,
                    content_hash: String::new(),
                    excerpt: truncate(&text, 300),
                    retrieval_reason: "vector_chunk".into(),
                    score,
                    trust_level: TrustLevel::UserNote,
                    citation_label: format!("[C{i}]"),
                    stale: false,
                }
            },
        )
        .collect();

    Ok(packets)
}

fn search_template(conn: &Connection, query: &str, limit: usize) -> AppResult<Vec<ContextPacket>> {
    // Search genre_templates by genre keyword match
    let mut stmt = match conn.prepare(
        "SELECT id, template_key, genre, subtype, structure, common_phrases,
                style_features, user_confirmed, usage_count
         FROM genre_templates
         WHERE genre LIKE ?1 OR subtype LIKE ?1
         ORDER BY user_confirmed DESC, usage_count DESC
         LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(_) => return Ok(vec![]), // genre_templates table may not exist yet
    };

    let pattern = format!("%{query}%");
    let rows = stmt.query_map(rusqlite::params![pattern, limit as i64], |row| {
        let id: i64 = row.get(0)?;
        let genre: String = row.get(2)?;
        let structure_str: String = row.get(4)?;
        let user_confirmed: i64 = row.get(7)?;
        let usage_count: i64 = row.get(8)?;
        Ok((id, genre, structure_str, user_confirmed, usage_count))
    })?;

    let packets: Vec<_> = rows
        .flatten()
        .enumerate()
        .map(
            |(i, (id, genre, structure_str, user_confirmed, usage_count))| {
                let structure: serde_json::Value =
                    serde_json::from_str(&structure_str).unwrap_or(serde_json::Value::Null);
                let excerpt = format!(
                    "文种: {genre} | 使用次数: {usage_count} | 结构: {}",
                    serde_json::to_string(&structure).unwrap_or_default()
                );
                let trust = if user_confirmed != 0 {
                    TrustLevel::UserNote
                } else {
                    TrustLevel::DerivedCache
                };
                ContextPacket {
                    id: format!("tmpl-{id}"),
                    source_type: SourceType::Template,
                    source_path: None,
                    title: format!("{genre} 模板"),
                    heading_path: None,
                    source_span: None,
                    content_hash: String::new(),
                    excerpt: truncate(&excerpt, 400),
                    retrieval_reason: "template_genre_match".into(),
                    score: if user_confirmed != 0 { 0.85 } else { 0.6 },
                    trust_level: trust,
                    citation_label: format!("[T{i}]"),
                    stale: false,
                }
            },
        )
        .collect();

    Ok(packets)
}

/// Weighted score fusion: normalize scores by layer, apply weights, deduplicate.
fn fuse_and_rank(packets: &mut Vec<ContextPacket>, max_results: usize) {
    // Layer weights: exact > regulation > user_note > anchor > chunk > template > graph
    fn layer_weight(p: &ContextPacket) -> f64 {
        match p.retrieval_reason.as_str() {
            r if r.starts_with("exact_") => 1.0,
            r if r.starts_with("vector_regulation") => 0.95,
            "fts_keyword_match" => 0.85,
            r if r.starts_with("vector_chunk") => 0.80,
            r if r.starts_with("vector_anchor") => 0.75,
            r if r.starts_with("template_") => 0.70,
            r if r.starts_with("graph_") => 0.60,
            _ => 0.50,
        }
    }

    // Apply weighted scores
    for p in packets.iter_mut() {
        let weight = layer_weight(p);
        p.score = (p.score * weight).min(1.0);
    }

    // Sort by weighted score descending
    packets.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Deduplicate: keep highest-scoring occurrence of each id
    packets.dedup_by(|a, b| a.id == b.id);
    packets.truncate(max_results);
}

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
