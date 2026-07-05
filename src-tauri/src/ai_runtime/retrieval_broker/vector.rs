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
    let query_vec = engine::embed_text(query)?;
    let blob = engine::f32_to_bytes(&query_vec);

    let mut stmt = conn.prepare(
        "SELECT vc.rowid, c.content, f.path, f.title, c.heading_path,
                c.source_start, c.source_end, c.content_hash, c.char_count, vc.distance
         FROM vec_chunks vc
         JOIN chunks c ON c.id = vc.rowid
         JOIN files f ON f.id = c.file_id
         WHERE vc.embedding MATCH ?1
           AND f.path <> '.classified'
           AND f.path NOT LIKE '.classified/%'
         ORDER BY vc.distance
         LIMIT ?2",
    )?;

    let rows = stmt.query_map(rusqlite::params![blob, limit as i64], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<i64>>(5)?,
            row.get::<_, Option<i64>>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, i64>(8)?,
            row.get::<_, f64>(9)?,
        ))
    })?;

    let packets: Vec<_> = rows
        .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!("vector chunk row parse failed: {e}");
                None
            }
        })
        .enumerate()
        .map(
            |(i, (rowid, text, path, title, heading, start, end, hash, _char_count, distance))| {
                let score = (1.0 - distance).max(0.0);
                let source_span = match (start, end) {
                    (Some(start), Some(end)) if start >= 0 && end >= start => Some(SourceSpan {
                        start: start as usize,
                        end: end as usize,
                    }),
                    _ => None,
                };
                ContextPacket {
                    id: format!("chunk-{rowid}"),
                    source_type: SourceType::Note,
                    source_path: Some(path),
                    title: title.clone(),
                    heading_path: heading,
                    source_span,
                    content_hash: hash.unwrap_or_default(),
                    excerpt: truncate(&text, 300),
                    retrieval_reason: "vector_chunk".into(),
                    score,
                    trust_level: TrustLevel::UserNote,
                    citation_label: format!("[C{i}]"),
                    stale: false,
                    web: None,
                    corpus: None,
                }
            },
        )
        .collect();

    Ok(packets)
}

pub(super) fn search_vector_anchors(
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
           AND f.path <> '.classified'
           AND f.path NOT LIKE '.classified/%'
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
        .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!("vector anchor row parse failed: {e}");
                None
            }
        })
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
                    web: None,
                    corpus: None,
                }
            },
        )
        .collect();

    Ok(packets)
}

pub(super) fn search_vector_regulations(
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
           AND f.path <> '.classified'
           AND f.path NOT LIKE '.classified/%'
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
        .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!("vector regulation row parse failed: {e}");
                None
            }
        })
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
                    web: None,
                    corpus: None,
                }
            },
        )
        .collect();

    Ok(packets)
}
