use rusqlite::Connection;

use crate::ai_runtime::{ContextPacket, SourceType, TrustLevel};
use crate::error::AppResult;

use super::truncate;

pub(super) fn search_template(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> AppResult<Vec<ContextPacket>> {
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
        .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!("template row parse failed: {e}");
                None
            }
        })
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
                    web: None,
                }
            },
        )
        .collect();

    Ok(packets)
}
