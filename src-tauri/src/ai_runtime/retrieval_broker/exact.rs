use std::sync::LazyLock;

use rusqlite::Connection;

use crate::ai_runtime::{ContextPacket, SourceType, TrustLevel};
use crate::error::AppResult;

use super::truncate;

static RE_REGULATION_ARTICLE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"《([^》]+)》\s*第([一二三四五六七八九十百千0-9]+)条")
        .expect("regulation article regex")
});

pub(super) fn search_exact_regulation(
    conn: &Connection,
    query: &str,
) -> AppResult<Vec<ContextPacket>> {
    let Some(caps) = RE_REGULATION_ARTICLE.captures(query) else {
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
        .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!("exact regulation row parse failed: {e}");
                None
            }
        })
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
                web: None,
                corpus: None,
            }
        })
        .collect();

    Ok(packets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_regulation_regex_matches() {
        let query = "《纪律处分条例》第六条怎么规定";

        assert!(RE_REGULATION_ARTICLE.is_match(query));
    }
}
