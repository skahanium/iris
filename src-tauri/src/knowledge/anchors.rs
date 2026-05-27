//! Semantic anchor extraction.
//!
//! Phase B: structural heuristics-based extraction (quote patterns,
//! definition patterns, decision patterns). No LLM dependency.
//! Phase C+ will add LLM-based refinement for low-confidence anchors.

use rusqlite::Connection;

use crate::embedding::engine::{embed_text, f32_to_bytes};
use crate::error::AppResult;
use crate::knowledge::{
    content_hash, make_anchor_key, EMBEDDING_DIM, EMBEDDING_MODEL, EXTRACTOR_VERSION,
};

// ─── Anchor Types ────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorType {
    Claim,
    Definition,
    Decision,
    RegulationRef,
    Fact,
}

impl AnchorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AnchorType::Claim => "claim",
            AnchorType::Definition => "definition",
            AnchorType::Decision => "decision",
            AnchorType::RegulationRef => "regulation_ref",
            AnchorType::Fact => "fact",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExtractedAnchor {
    pub anchor_key: String,
    pub anchor_type: AnchorType,
    pub content: String,
    pub heading_path: Option<String>,
    pub source_start: usize,
    pub source_end: usize,
    pub paragraph_index: Option<usize>,
    pub content_hash: String,
    pub confidence: f64,
}

// ─── Extraction ──────────────────────────────────────────

/// Extract anchors from note text using structural heuristics.
pub fn extract_anchors(
    file_path: &str,
    raw_text: &str,
) -> Vec<ExtractedAnchor> {
    let lines: Vec<&str> = raw_text.lines().collect();
    let headings = collect_headings(&lines);
    let mut anchors = Vec::new();
    let mut abs_offset = 0usize;
    let mut para_index = 0usize;

    for line in &lines {
        let line_start = abs_offset;
        let line_end = abs_offset + line.len();
        let trimmed = line.trim();

        if trimmed.is_empty() || is_heading_line(trimmed) {
            abs_offset = line_end + 1; // +1 for newline
            continue;
        }

        para_index += 1;
        let heading = closest_heading(&headings, line_start);
        let confidence_base = 0.7;

        // Pattern 1: Decision markers
        if let Some(anchor) = try_decision(line, file_path, line_start, line_end, para_index, &heading, confidence_base) {
            anchors.push(anchor);
        }

        // Pattern 2: Definition / explanation markers
        if let Some(anchor) = try_definition(line, file_path, line_start, line_end, para_index, &heading, confidence_base) {
            anchors.push(anchor);
        }

        // Pattern 3: Regulation references
        if let Some(anchor) = try_regulation_ref(line, file_path, line_start, line_end, para_index, &heading, 0.9) {
            anchors.push(anchor);
        }

        // Pattern 4: Fact / data patterns
        if let Some(anchor) = try_fact(line, file_path, line_start, line_end, para_index, &heading, confidence_base) {
            anchors.push(anchor);
        }

        // Pattern 5: Claim — sentences with judgment keywords
        if let Some(anchor) = try_claim(line, file_path, line_start, line_end, para_index, &heading, 0.6) {
            anchors.push(anchor);
        }

        abs_offset = line_end + 1;
    }

    anchors
}

// ─── Heading Tracking ────────────────────────────────────

struct HeadingInfo {
    text: String,
    offset: usize,
}

fn collect_headings(lines: &[&str]) -> Vec<HeadingInfo> {
    let mut headings = Vec::new();
    let mut offset = 0usize;
    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            headings.push(HeadingInfo {
                text: trimmed.trim_start_matches('#').trim().to_string(),
                offset,
            });
        }
        offset += line.len() + 1;
    }
    headings
}

fn is_heading_line(line: &str) -> bool {
    line.starts_with('#')
}

fn closest_heading(headings: &[HeadingInfo], offset: usize) -> Option<String> {
    headings
        .iter()
        .rev()
        .find(|h| h.offset < offset)
        .map(|h| h.text.clone())
}

// ─── Pattern Matchers ────────────────────────────────────

fn try_decision(
    line: &str, path: &str, start: usize, end: usize,
    para: usize, heading: &Option<String>, conf: f64,
) -> Option<ExtractedAnchor> {
    let trimmed = line.trim();
    let decision_markers = ["经研究", "决定", "综上所述", "会议决定", "同意", "批准", "不予"];
    if decision_markers.iter().any(|m| trimmed.contains(m)) && trimmed.chars().count() > 10 {
        let content = trimmed.to_string();
        let hash = content_hash(&content);
        let key = make_anchor_key(path, start, end, &content);
        Some(ExtractedAnchor {
            anchor_key: key,
            anchor_type: AnchorType::Decision,
            content,
            heading_path: heading.clone(),
            source_start: start,
            source_end: end,
            paragraph_index: Some(para),
            content_hash: hash,
            confidence: conf,
        })
    } else {
        None
    }
}

fn try_definition(
    line: &str, path: &str, start: usize, end: usize,
    para: usize, heading: &Option<String>, conf: f64,
) -> Option<ExtractedAnchor> {
    let trimmed = line.trim();
    let def_patterns = ["是指", "定义为", "指的是", "即", "所谓"];
    if def_patterns.iter().any(|m| trimmed.contains(m)) && trimmed.chars().count() > 15 {
        let content = trimmed.to_string();
        let hash = content_hash(&content);
        let key = make_anchor_key(path, start, end, &content);
        Some(ExtractedAnchor {
            anchor_key: key,
            anchor_type: AnchorType::Definition,
            content,
            heading_path: heading.clone(),
            source_start: start,
            source_end: end,
            paragraph_index: Some(para),
            content_hash: hash,
            confidence: conf,
        })
    } else {
        None
    }
}

fn try_regulation_ref(
    line: &str, path: &str, start: usize, end: usize,
    para: usize, heading: &Option<String>, conf: f64,
) -> Option<ExtractedAnchor> {
    let trimmed = line.trim();
    let re = regex::Regex::new(r"《[^》]+》第[一二三四五六七八九十百千0-9]+条").unwrap();
    if re.is_match(trimmed) {
        let content = trimmed.to_string();
        let hash = content_hash(&content);
        let key = make_anchor_key(path, start, end, &content);
        Some(ExtractedAnchor {
            anchor_key: key,
            anchor_type: AnchorType::RegulationRef,
            content,
            heading_path: heading.clone(),
            source_start: start,
            source_end: end,
            paragraph_index: Some(para),
            content_hash: hash,
            confidence: conf,
        })
    } else {
        None
    }
}

fn try_fact(
    line: &str, path: &str, start: usize, end: usize,
    para: usize, heading: &Option<String>, conf: f64,
) -> Option<ExtractedAnchor> {
    let trimmed = line.trim();
    // Contains percentage, year-range, or numeric data with units
    let has_data = regex::Regex::new(r"\d+[\.\d]*%|\d{4}年|\d+万|\d+亿|\d+个|\d+项").unwrap();
    if has_data.is_match(trimmed) && trimmed.chars().count() > 10 {
        let content = trimmed.to_string();
        let hash = content_hash(&content);
        let key = make_anchor_key(path, start, end, &content);
        Some(ExtractedAnchor {
            anchor_key: key,
            anchor_type: AnchorType::Fact,
            content,
            heading_path: heading.clone(),
            source_start: start,
            source_end: end,
            paragraph_index: Some(para),
            content_hash: hash,
            confidence: conf,
        })
    } else {
        None
    }
}

fn try_claim(
    line: &str, path: &str, start: usize, end: usize,
    para: usize, heading: &Option<String>, conf: f64,
) -> Option<ExtractedAnchor> {
    let trimmed = line.trim();
    let claim_markers = ["应当", "必须", "不得", "禁止", "需要", "要坚持", "要始终", "必须坚持", "关键在于"];
    if claim_markers.iter().any(|m| trimmed.contains(m)) && trimmed.chars().count() > 15 {
        let content = trimmed.to_string();
        let hash = content_hash(&content);
        let key = make_anchor_key(path, start, end, &content);
        Some(ExtractedAnchor {
            anchor_key: key,
            anchor_type: AnchorType::Claim,
            content,
            heading_path: heading.clone(),
            source_start: start,
            source_end: end,
            paragraph_index: Some(para),
            content_hash: hash,
            confidence: conf,
        })
    } else {
        None
    }
}

// ─── Index into Database ─────────────────────────────────

/// Index extracted anchors into the database.
pub fn index_anchors(
    conn: &Connection,
    file_id: i64,
    anchors: &[ExtractedAnchor],
) -> AppResult<usize> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut indexed = 0usize;

    for anchor in anchors {
        // Generate embedding
        let embedding = embed_text(&anchor.content)?;
        let blob = f32_to_bytes(&embedding);

        conn.execute(
            "INSERT OR IGNORE INTO semantic_anchors
             (anchor_key, file_id, anchor_type, content, heading_path,
              source_start, source_end, paragraph_index, content_hash,
              extractor_version, embedding_model, embedding_dim, confidence,
              created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)",
            rusqlite::params![
                anchor.anchor_key,
                file_id,
                anchor.anchor_type.as_str(),
                anchor.content,
                anchor.heading_path,
                anchor.source_start as i64,
                anchor.source_end as i64,
                anchor.paragraph_index,
                anchor.content_hash,
                EXTRACTOR_VERSION,
                EMBEDDING_MODEL,
                EMBEDDING_DIM,
                anchor.confidence,
                now,
            ],
        )?;

        // Insert into vec table
        let rowid = conn.last_insert_rowid();
        if rowid > 0 {
            conn.execute(
                "INSERT OR IGNORE INTO vec_anchors (rowid, embedding) VALUES (?1, ?2)",
                rusqlite::params![rowid, blob],
            )?;
            indexed += 1;
        }
    }

    Ok(indexed)
}

/// Delete all anchors for a file (before re-indexing).
pub fn delete_anchors_for_file(conn: &Connection, file_id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM semantic_anchors WHERE file_id = ?1", [file_id])?;
    Ok(())
}

// ─── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_NOTE: &str = r#"# 纪检监察工作会议纪要

## 会议精神

会议强调，必须坚持全面从严治党不放松，始终保持惩治腐败高压态势。

经研究，决定成立专项督查组，对各单位落实中央八项规定精神情况进行全面检查。

"四种形态"是指经常开展批评和自我批评、约谈函询，让"红红脸、出出汗"成为常态；党纪轻处分、组织调整成为违纪处理的大多数；党纪重处分、重大职务调整的成为少数；严重违纪涉嫌违法立案审查的成为极少数。

根据《中国共产党纪律处分条例》第六条，本条例适用于违犯党纪应当受到党纪责任追究的党组织和党员。

## 数据统计

2024年全市纪检监察机关共立案1234件，处分1567人，同比增长12.3%。
"#;

    #[test]
    fn extract_anchors_finds_decisions() {
        let anchors = extract_anchors("test.md", SAMPLE_NOTE);
        let decisions: Vec<_> = anchors.iter().filter(|a| a.anchor_type == AnchorType::Decision).collect();
        assert!(!decisions.is_empty(), "should find at least one decision");
        assert!(decisions.iter().any(|d| d.content.contains("决定成立")));
    }

    #[test]
    fn extract_anchors_finds_definitions() {
        let anchors = extract_anchors("test.md", SAMPLE_NOTE);
        let defs: Vec<_> = anchors.iter().filter(|a| a.anchor_type == AnchorType::Definition).collect();
        assert!(!defs.is_empty());
        assert!(defs.iter().any(|d| d.content.contains("是指")));
    }

    #[test]
    fn extract_anchors_finds_regulation_refs() {
        let anchors = extract_anchors("test.md", SAMPLE_NOTE);
        let refs: Vec<_> = anchors.iter().filter(|a| a.anchor_type == AnchorType::RegulationRef).collect();
        assert!(!refs.is_empty());
    }

    #[test]
    fn extract_anchors_finds_claims() {
        let anchors = extract_anchors("test.md", SAMPLE_NOTE);
        let claims: Vec<_> = anchors.iter().filter(|a| a.anchor_type == AnchorType::Claim).collect();
        assert!(!claims.is_empty());
    }

    #[test]
    fn extract_anchors_finds_facts() {
        let anchors = extract_anchors("test.md", SAMPLE_NOTE);
        let facts: Vec<_> = anchors.iter().filter(|a| a.anchor_type == AnchorType::Fact).collect();
        assert!(!facts.is_empty());
    }

    #[test]
    fn anchor_keys_are_stable() {
        let anchors1 = extract_anchors("test.md", SAMPLE_NOTE);
        let anchors2 = extract_anchors("test.md", SAMPLE_NOTE);
        let keys1: Vec<_> = anchors1.iter().map(|a| &a.anchor_key).collect();
        let keys2: Vec<_> = anchors2.iter().map(|a| &a.anchor_key).collect();
        assert_eq!(keys1, keys2);
    }
}
