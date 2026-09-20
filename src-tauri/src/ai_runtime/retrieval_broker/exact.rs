use std::sync::LazyLock;

use rusqlite::Connection;

use crate::ai_runtime::{ContextPacket, SourceType, TrustLevel};
use crate::error::AppResult;

use super::truncate;

static RE_REGULATION_ARTICLE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"《([^》]+)》\s*第?([一二三四五六七八九十百千万0-9]+)条(?:第?([一二三四五六七八九十百千万0-9]+)款)?",
    )
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
    let wanted_paragraph = caps
        .get(3)
        .map(|capture| format!("第{}款", capture.as_str()));
    if article_query_has_unbound_paragraph(query, &caps) {
        return Ok(vec![]);
    }

    let mut stmt = conn.prepare(
        "SELECT ri.id, ri.content, f.path, f.title, ri.regulation_name,
                ri.article, ri.paragraph
         FROM regulation_index ri
         JOIN files f ON f.id = ri.file_id
         WHERE ri.regulation_name = ?1 AND ri.article = ?2
           AND f.path <> '.classified'
           AND f.path NOT LIKE '.classified/%'
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
        .filter(|(_, _, _, _, _, _, paragraph)| {
            paragraph_matches(wanted_paragraph.as_deref(), paragraph.as_deref())
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

fn paragraph_matches(wanted: Option<&str>, actual: Option<&str>) -> bool {
    let Some(wanted) = wanted else {
        return true;
    };
    actual.is_some_and(|actual| actual == wanted || actual == wanted.trim_start_matches('第'))
}

fn article_query_has_unbound_paragraph(query: &str, caps: &regex::Captures<'_>) -> bool {
    caps.get(3).is_none() && query.contains('款')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_regulation_regex_matches() {
        let query = "《纪律处分条例》第六条怎么规定";

        assert!(RE_REGULATION_ARTICLE.is_match(query));
    }

    #[test]
    fn exact_regulation_regex_captures_paragraph() {
        let caps = RE_REGULATION_ARTICLE
            .captures("《纪律处分条例》第六条第一款")
            .expect("paragraph query");
        assert_eq!(&caps[1], "纪律处分条例");
        assert_eq!(&caps[2], "六");
        assert_eq!(&caps[3], "一");
    }

    #[test]
    fn paragraph_mismatch_does_not_accept_the_whole_article() {
        assert!(paragraph_matches(Some("第一款"), Some("第一款")));
        assert!(paragraph_matches(Some("第一款"), Some("一款")));
        assert!(!paragraph_matches(Some("第一款"), Some("第二款")));
        assert!(!paragraph_matches(Some("第一款"), None));
        assert!(paragraph_matches(None, Some("第二款")));
    }

    #[test]
    fn leftover_paragraph_without_a_capture_does_not_return_the_whole_article() {
        let query = "《纪律处分条例》第六条以及第一款";
        let caps = RE_REGULATION_ARTICLE
            .captures(query)
            .expect("article still matches");
        assert!(article_query_has_unbound_paragraph(query, &caps));
        let bound = RE_REGULATION_ARTICLE
            .captures("《纪律处分条例》第六条第一款")
            .expect("bound paragraph");
        assert!(!article_query_has_unbound_paragraph(
            "《纪律处分条例》第六条第一款",
            &bound
        ));
        assert!(!article_query_has_unbound_paragraph(
            "《纪律处分条例》第六条",
            &RE_REGULATION_ARTICLE
                .captures("《纪律处分条例》第六条")
                .expect("article only")
        ));
    }
}
