use rusqlite::Connection;

use crate::ai_runtime::{ContextPacket, SourceType, TrustLevel};
use crate::error::AppResult;

pub(super) fn search_graph_neighbors(
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
        .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!("graph neighbor row parse failed: {e}");
                None
            }
        })
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
                web: None,
            },
        )
        .collect();

    Ok(packets)
}
