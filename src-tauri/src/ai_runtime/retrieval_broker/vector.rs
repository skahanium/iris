use rusqlite::Connection;

use crate::ai_runtime::{ContextPacket, SourceSpan, SourceType, TrustLevel};
use crate::embedding::engine;
use crate::error::AppResult;

use super::truncate;

pub(super) fn search_vector_chunks(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> AppResult<Vec<ContextPacket>> {
    if !engine::embedding_generation_ready(conn)? {
        return Ok(Vec::new());
    }
    let query_embedding = engine::embed_query(query)?;
    let mut statement = conn.prepare(
        "SELECT c.id, c.content, f.path, f.title, c.heading_path,
                c.source_start, c.source_end, c.content_hash, ce.embedding
         FROM chunk_embeddings_v2 AS ce
         INNER JOIN chunks AS c ON c.id = ce.chunk_id
         INNER JOIN files AS f ON f.id = c.file_id
         WHERE f.path <> '.classified'
           AND f.path NOT LIKE '.classified/%'",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<i64>>(5)?,
            row.get::<_, Option<i64>>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, Vec<u8>>(8)?,
        ))
    })?;

    let mut packets = Vec::new();
    for row in rows.flatten() {
        let (chunk_id, content, path, title, heading_path, start, end, content_hash, blob) = row;
        let embedding = engine::bytes_to_f32(&blob);
        if embedding.len() != engine::EMBEDDING_DIMENSION {
            tracing::warn!(
                chunk_id,
                dimensions = embedding.len(),
                "skipping invalid v2 vector row"
            );
            continue;
        }
        let source_span = match (start, end) {
            (Some(start), Some(end)) if start >= 0 && end >= start => Some(SourceSpan {
                start: start as usize,
                end: end as usize,
            }),
            _ => None,
        };
        packets.push(ContextPacket {
            id: format!("chunk-{chunk_id}"),
            source_type: SourceType::Note,
            source_path: Some(path),
            title,
            heading_path,
            source_span,
            content_hash: content_hash.unwrap_or_default(),
            excerpt: truncate(&content, 300),
            retrieval_reason: "vector_chunk".to_string(),
            score: engine::cosine_similarity(&query_embedding, &embedding) as f64,
            trust_level: TrustLevel::UserNote,
            citation_label: format!("[C{chunk_id}]"),
            stale: false,
            web: None,
            corpus: None,
        });
    }
    packets.sort_by(|left, right| right.score.total_cmp(&left.score));
    packets.truncate(limit);
    Ok(packets)
}
pub(super) fn search_vector_anchors(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> AppResult<Vec<ContextPacket>> {
    search_structured_vectors(
        conn,
        query,
        limit,
        "SELECT a.id, a.content, f.path, f.title, a.heading_path, a.source_start, a.source_end, a.content_hash, e.embedding, a.confidence
         FROM semantic_anchor_embeddings_v2 AS e
         INNER JOIN semantic_anchors AS a ON a.id = e.anchor_id
         INNER JOIN files AS f ON f.id = a.file_id
         WHERE f.path <> '.classified' AND f.path NOT LIKE '.classified/%'",
        "anchor",
        SourceType::Anchor,
        TrustLevel::UserNote,
    )
}

pub(super) fn search_vector_regulations(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> AppResult<Vec<ContextPacket>> {
    search_structured_vectors(
        conn,
        query,
        limit,
        "SELECT r.id, r.content, f.path, f.title, r.article, r.source_start, r.source_end, r.content_hash, e.embedding, 1.0
         FROM regulation_embeddings_v2 AS e
         INNER JOIN regulation_index AS r ON r.id = e.regulation_id
         INNER JOIN files AS f ON f.id = r.file_id
         WHERE f.path <> '.classified' AND f.path NOT LIKE '.classified/%'",
        "regulation",
        SourceType::Regulation,
        TrustLevel::UserNote,
    )
}

fn search_structured_vectors(
    conn: &Connection,
    query: &str,
    limit: usize,
    sql: &str,
    kind: &str,
    source_type: SourceType,
    trust_level: TrustLevel,
) -> AppResult<Vec<ContextPacket>> {
    if !engine::embedding_generation_ready(conn)? {
        return Ok(Vec::new());
    }
    let query_embedding = engine::embed_query(query)?;
    let mut statement = conn.prepare(sql)?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, Vec<u8>>(8)?,
            row.get::<_, f64>(9)?,
        ))
    })?;
    let mut packets = Vec::new();
    for row in rows.flatten() {
        let (id, content, path, title, heading_path, start, end, hash, blob, confidence) = row;
        let embedding = engine::bytes_to_f32(&blob);
        if embedding.len() != engine::EMBEDDING_DIMENSION
            || start < 0
            || end < start
            || hash.is_empty()
        {
            continue;
        }
        packets.push(ContextPacket {
            id: format!("{kind}-{id}"),
            source_type,
            source_path: Some(path),
            title,
            heading_path,
            source_span: Some(SourceSpan {
                start: start as usize,
                end: end as usize,
            }),
            content_hash: hash,
            excerpt: truncate(&content, 400),
            retrieval_reason: format!("vector_{kind}"),
            score: (engine::cosine_similarity(&query_embedding, &embedding) as f64
                * confidence.clamp(0.0, 1.0)),
            trust_level,
            citation_label: format!("[V{id}]"),
            stale: false,
            web: None,
            corpus: None,
        });
    }
    packets.sort_by(|left, right| right.score.total_cmp(&left.score));
    packets.truncate(limit);
    Ok(packets)
}
