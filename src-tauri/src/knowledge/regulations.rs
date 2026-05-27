//! Regulation clause parsing and indexing.
//!
//! Two-phase approach:
//! Phase 1: Rust regex parses "第X条" / "第X款" boundaries (no LLM cost)
//! Phase 2: Per-clause embedding + keyword extraction via LLM (batch, optional)

use regex::Regex;
use rusqlite::Connection;
use std::sync::LazyLock;

use crate::embedding::engine::{embed_text, f32_to_bytes};
use crate::error::AppResult;
use crate::knowledge::{content_hash, EMBEDDING_DIM, EMBEDDING_MODEL, EXTRACTOR_VERSION};

// ─── Regex Patterns ──────────────────────────────────────

static RE_ARTICLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|\n)\s*第[一二三四五六七八九十百千0-9]+条\b").expect("article regex")
});

static RE_PARAGRAPH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|\n)\s*[（(]?[一二三四五六七八九十0-9]+[）)]?\s*款?").expect("paragraph regex")
});

static RE_REGULATION_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"《([^》]+)》").expect("regulation name regex"));

// ─── Data Types ──────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RegulationClause {
    pub regulation_name: String,
    pub chapter: Option<String>,
    pub section: Option<String>,
    pub article: String,
    pub paragraph: Option<String>,
    pub content: String,
    pub source_start: usize,
    pub source_end: usize,
    pub content_hash: String,
}

#[derive(Debug, Clone)]
pub struct ParseResult {
    pub clauses: Vec<RegulationClause>,
    pub regulation_name: String,
    pub chapter_names: Vec<String>,
}

// ─── Phase 1: Structural Parsing ─────────────────────────

/// Parse a regulation `.md` file into structured clauses.
/// Uses Rust regex only — no LLM cost.
pub fn parse_regulation_structure(file_path: &str, raw_text: &str) -> ParseResult {
    let regulation_name =
        extract_regulation_name(raw_text).unwrap_or_else(|| file_name_from_path(file_path));

    let article_matches: Vec<_> = RE_ARTICLE.find_iter(raw_text).collect();
    let mut clauses = Vec::with_capacity(article_matches.len());
    let mut chapter_names = Vec::new();

    // Track current chapter/section context
    let mut current_chapter: Option<String> = None;
    let mut current_section: Option<String> = None;

    for (i, article_match) in article_matches.iter().enumerate() {
        let article_start = article_match.start();
        let article_end = if i + 1 < article_matches.len() {
            article_matches[i + 1].start()
        } else {
            raw_text.len()
        };

        let article_text = &raw_text[article_start..article_end];
        let article_num = article_match.as_str().trim().to_string();

        // Detect chapter/section from preceding text
        let preceding = if article_start > 0 {
            &raw_text[..article_start]
        } else {
            ""
        };
        update_context(
            preceding,
            &mut current_chapter,
            &mut current_section,
            &mut chapter_names,
        );

        // Split article into paragraphs if present
        let para_splits: Vec<_> = RE_PARAGRAPH.find_iter(article_text).collect();

        if para_splits.len() <= 1 {
            // Single paragraph — whole article is one clause
            let content = article_text.trim().to_string();
            clauses.push(RegulationClause {
                regulation_name: regulation_name.clone(),
                chapter: current_chapter.clone(),
                section: current_section.clone(),
                article: article_num,
                paragraph: None,
                content_hash: content_hash(&content),
                content,
                source_start: article_start,
                source_end: article_end,
            });
        } else {
            // Multi-paragraph article — create a clause per paragraph
            for (j, para_match) in para_splits.iter().enumerate() {
                let para_start = article_start + para_match.start();
                let para_end = if j + 1 < para_splits.len() {
                    article_start + para_splits[j + 1].start()
                } else {
                    article_end
                };
                let para_text = raw_text[para_start..para_end].trim().to_string();
                let para_num = para_match.as_str().trim().to_string();

                clauses.push(RegulationClause {
                    regulation_name: regulation_name.clone(),
                    chapter: current_chapter.clone(),
                    section: current_section.clone(),
                    article: article_num.clone(),
                    paragraph: Some(para_num),
                    content_hash: content_hash(&para_text),
                    content: para_text,
                    source_start: para_start,
                    source_end: para_end,
                });
            }
        }
    }

    ParseResult {
        clauses,
        regulation_name,
        chapter_names,
    }
}

fn extract_regulation_name(text: &str) -> Option<String> {
    RE_REGULATION_NAME
        .captures(text)
        .and_then(|caps| caps.get(1))
        .map(|m| format!("《{}》", m.as_str()))
}

fn file_name_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string()
}

fn update_context(
    preceding: &str,
    chapter: &mut Option<String>,
    section: &mut Option<String>,
    chapter_names: &mut Vec<String>,
) {
    // Detect chapter headings: 第X章 ... or 第一章 ...
    let ch_re = Regex::new(r"第[一二三四五六七八九十百千0-9]+章\s*(.+)").unwrap();
    if let Some(caps) = ch_re.captures(preceding.split('\n').next_back().unwrap_or("")) {
        *chapter = Some(caps[0].trim().to_string());
        if let Some(name) = caps.get(1) {
            chapter_names.push(name.as_str().to_string());
        }
        *section = None; // new chapter resets section
    }

    // Detect section headings within a chapter
    let sec_re = Regex::new(r"第[一二三四五六七八九十百千0-9]+节\s*(.+)").unwrap();
    if let Some(caps) = sec_re.captures(preceding.split('\n').next_back().unwrap_or("")) {
        *section = Some(caps[0].trim().to_string());
    }
}

// ─── Phase 2: Index to Database ───────────────────────────

/// Index parsed regulation clauses into the database.
/// Each clause gets an embedding for semantic search.
pub fn index_regulation_clauses(
    conn: &Connection,
    file_id: i64,
    clauses: &[RegulationClause],
) -> AppResult<usize> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut indexed = 0usize;

    for clause in clauses {
        // Generate embedding
        let embedding = embed_text(&clause.content)?;
        let blob = f32_to_bytes(&embedding);

        // Build keywords: extract from content (simple heuristic — LLM batch in Phase C)
        let keywords = extract_keywords_heuristic(&clause.content);

        conn.execute(
            "INSERT INTO regulation_index
             (file_id, regulation_name, issuer, version_label, chapter, section,
              article, paragraph, content, keywords, source_start, source_end,
              content_hash, parser_version, embedding_model, embedding_dim, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            rusqlite::params![
                file_id,
                clause.regulation_name,
                None::<String>, // issuer — Phase C+
                None::<String>, // version_label — Phase C+
                clause.chapter,
                clause.section,
                clause.article,
                clause.paragraph,
                clause.content,
                keywords,
                clause.source_start as i64,
                clause.source_end as i64,
                clause.content_hash,
                EXTRACTOR_VERSION,
                EMBEDDING_MODEL,
                EMBEDDING_DIM,
                now,
            ],
        )?;

        // Insert into vec table
        let rowid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO vec_regulations (rowid, embedding) VALUES (?1, ?2)",
            rusqlite::params![rowid, blob],
        )?;

        indexed += 1;
    }

    Ok(indexed)
}

/// Heuristic keyword extraction — no LLM.
/// Phase C+ will add LLM batch refinement.
fn extract_keywords_heuristic(content: &str) -> String {
    // Extract quoted terms, proper nouns (《》), and common legal terms
    let mut keywords = Vec::new();

    // Terms in guillemets
    let re_term = Regex::new(r"《([^》]{2,20})》").unwrap();
    for cap in re_term.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            keywords.push(m.as_str().to_string());
        }
    }

    // Quoted short phrases
    let re_quote = Regex::new(r#"[""]([^""]{2,20})[""]"#).unwrap();
    for cap in re_quote.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            keywords.push(m.as_str().to_string());
        }
    }

    keywords.dedup();
    if keywords.is_empty() {
        String::new()
    } else {
        keywords.join(",")
    }
}

/// Re-index all regulations in the vault.
pub fn reindex_all_regulations(
    conn: &Connection,
    vault_path: &std::path::Path,
) -> AppResult<usize> {
    // Clear existing regulation index
    conn.execute("DELETE FROM regulation_index", [])?;

    let mut total = 0usize;
    let file_list: Vec<(i64, String)> = {
        let mut stmt = conn.prepare("SELECT id, path FROM files")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.flatten().collect()
    };

    for (file_id, path) in file_list {
        let abs = vault_path.join(&path);
        if !abs.exists() {
            continue;
        }
        let text = std::fs::read_to_string(&abs)?;
        let result = parse_regulation_structure(&path, &text);
        if result.clauses.is_empty() {
            continue;
        }
        total += index_regulation_clauses(conn, file_id, &result.clauses)?;
    }

    Ok(total)
}

// ─── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_REGULATION: &str = r#"# 《中国共产党纪律处分条例》

## 第一编 总则

### 第一章 指导思想、原则和适用范围

第一条 为了维护党章和其他党内法规，严肃党的纪律，纯洁党的组织，保障党员民主权利，教育党员遵纪守法，维护党的团结统一，保证党的路线、方针、政策、决议和国家法律法规的贯彻执行，根据《中国共产党章程》，制定本条例。

第二条 党的纪律建设必须坚持以马克思列宁主义、毛泽东思想、邓小平理论、"三个代表"重要思想、科学发展观为指导，坚持和加强党的全面领导，坚决维护习近平总书记党中央的核心、全党的核心地位，坚决维护党中央权威和集中统一领导。

### 第二章 违纪与纪律处分

第六条 本条例适用于违犯党纪应当受到党纪责任追究的党组织和党员。

第七条 党组织和党员违反党章和其他党内法规，违反国家法律法规，违反党和国家政策，违反社会主义道德，危害党、国家和人民利益的行为，依照规定应当给予纪律处理或者处分的，都必须受到追究。
"#;

    #[test]
    fn parse_extracts_regulation_name() {
        let result = parse_regulation_structure("test.md", SAMPLE_REGULATION);
        assert_eq!(result.regulation_name, "《中国共产党纪律处分条例》");
    }

    #[test]
    fn parse_extracts_articles() {
        let result = parse_regulation_structure("test.md", SAMPLE_REGULATION);
        assert!(
            result.clauses.len() >= 4,
            "expected >= 4 clauses, got {}",
            result.clauses.len()
        );

        let first = &result.clauses[0];
        assert!(first.article.contains("第一条"));
        assert!(first.content.contains("维护党章"));

        let seventh = result.clauses.iter().find(|c| c.article.contains("第七条"));
        assert!(seventh.is_some());
    }

    #[test]
    fn parse_includes_chapter_context() {
        let result = parse_regulation_structure("test.md", SAMPLE_REGULATION);
        let ch1_clause = result
            .clauses
            .iter()
            .find(|c| c.article.contains("第一条"))
            .unwrap();
        assert!(ch1_clause
            .chapter
            .as_ref()
            .map_or(false, |c| c.contains("第一章")));
    }

    #[test]
    #[ignore] // Requires fastembed AllMiniLML6V2 model download (~80 MB)
    fn index_and_retrieve() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        // Need core tables for FK
        crate::storage::migrate::migrate_up(&conn).unwrap();

        // Insert a file record
        conn.execute(
            "INSERT INTO files (path, title, content_hash, created_at, updated_at)
             VALUES ('law.md', 'Law', 'hash1', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();

        let result = parse_regulation_structure("law.md", SAMPLE_REGULATION);
        let count = index_regulation_clauses(&conn, 1, &result.clauses).unwrap();
        assert!(count > 0);

        // Verify vec_regulations has entries
        let vec_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM vec_regulations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(vec_count as usize, count);
    }

    #[test]
    fn heuristic_keywords_extracts_terms() {
        let content = "根据《中国共产党章程》和《问责条例》的规定，应当予以问责。";
        let kw = extract_keywords_heuristic(content);
        assert!(kw.contains("中国共产党章程"));
        assert!(kw.contains("问责条例"));
    }
}
