use std::sync::LazyLock;

use rusqlite::Connection;

use crate::ai_runtime::{ContextPacket, SourceType, TrustLevel};
use crate::error::AppResult;
use crate::knowledge::regulations::normalize_regulation_label;

use super::truncate;

static RE_REGULATION_ARTICLE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"《([^》]+)》\s*第?\s*([一二三四五六七八九十百千万0-9]+)\s*条(?:\s*第?\s*([一二三四五六七八九十百千万0-9]+)\s*款)?",
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
    let article = normalize_regulation_label(&caps[2], '条').expect("article ordinal");
    let wanted_paragraph = caps
        .get(3)
        .and_then(|capture| normalize_regulation_label(capture.as_str(), '款'));
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
         ORDER BY f.path, ri.id",
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
        .take(5)
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
    actual
        .and_then(|actual| normalize_regulation_label(actual, '款'))
        .is_some_and(|actual| {
            normalize_regulation_label(wanted, '款').as_deref() == Some(actual.as_str())
        })
}

fn article_query_has_unbound_paragraph(query: &str, caps: &regex::Captures<'_>) -> bool {
    caps.get(3).is_none()
        && caps
            .get(0)
            .is_some_and(|article| query[article.end()..].contains('款'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review_regulation_fixture() -> Connection {
        let conn = Connection::open_in_memory().expect("sqlite");
        conn.execute_batch(
            "CREATE TABLE files(id INTEGER PRIMARY KEY, path TEXT, title TEXT);
            CREATE TABLE regulation_index(id INTEGER PRIMARY KEY, file_id INTEGER,
            regulation_name TEXT, article TEXT, paragraph TEXT, content TEXT);
            INSERT INTO files VALUES(1, 'law.md', '条例'), (2, '.classified/law.md', '保密');",
        )
        .expect("fixture schema");
        conn
    }

    #[test]
    fn review_regression_ef_law_name_with_kuan_is_not_a_paragraph() {
        let conn = review_regulation_fixture();
        conn.execute(
            "INSERT INTO regulation_index VALUES(1,1,'《存款条例》','第六条',NULL,'公开条文')",
            [],
        )
        .unwrap();
        let packets = search_exact_regulation(&conn, "《存款条例》第六条").unwrap();
        assert_eq!(packets.len(), 1);
    }

    #[test]
    fn review_regression_ef_paragraph_filter_precedes_limit_and_accepts_whitespace() {
        let conn = review_regulation_fixture();
        for id in 1..=7 {
            conn.execute(
                "INSERT INTO regulation_index VALUES(?1,1,'《条例》','第六条',?2,?3)",
                rusqlite::params![
                    id,
                    if id < 6 { "第二款" } else { "一款" },
                    format!("clause-{id}")
                ],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO regulation_index VALUES(8,2,'《条例》','第六条','第一款','保密内容')",
            [],
        )
        .unwrap();
        for query in ["《条例》第六条第一款", "《条例》第六条 \n 第 一 款"] {
            let packets = search_exact_regulation(&conn, query).unwrap();
            assert_eq!(packets.len(), 2, "{query}");
            assert_eq!(packets[0].id, "exact-6");
            assert_eq!(packets[1].id, "exact-7");
            assert!(packets
                .iter()
                .all(|packet| packet.source_path.as_deref() == Some("law.md")));
        }
    }

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
