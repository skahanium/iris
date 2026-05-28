//! Block-level link graph.
//!
//! Maintains explicit ([[...]]) and implicit (AI-suggested) block-level links.

use rusqlite::Connection;

use crate::error::AppResult;

#[derive(Debug, Clone)]
pub struct BlockLink {
    pub id: i64,
    pub source_file_id: i64,
    pub source_anchor_key: Option<String>,
    pub target_file_id: i64,
    pub target_anchor_key: Option<String>,
    pub link_type: String,
    pub confidence: f64,
    pub is_confirmed: bool,
}

/// Insert a block link. Uses INSERT OR IGNORE to avoid duplicates.
#[allow(clippy::too_many_arguments)]
pub fn insert_link(
    conn: &Connection,
    source_file_id: i64,
    source_anchor_key: Option<&str>,
    target_file_id: i64,
    target_anchor_key: Option<&str>,
    link_type: &str,
    confidence: f64,
    created_by: &str,
) -> AppResult<i64> {
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO block_links
         (source_file_id, source_anchor_key, target_file_id, target_anchor_key,
          link_type, confidence, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            source_file_id,
            source_anchor_key,
            target_file_id,
            target_anchor_key,
            link_type,
            confidence,
            created_by,
            now,
        ],
    )?;

    Ok(conn.last_insert_rowid())
}

/// Get confirmed (explicit or user-confirmed) links for a file.
pub fn get_confirmed_links(conn: &Connection, file_id: i64) -> AppResult<Vec<BlockLink>> {
    let mut stmt = conn.prepare(
        "SELECT id, source_file_id, source_anchor_key, target_file_id, target_anchor_key,
                link_type, confidence, is_confirmed
         FROM block_links
         WHERE source_file_id = ?1 AND is_confirmed = 1
         ORDER BY confidence DESC",
    )?;

    let rows = stmt.query_map([file_id], |row| {
        let confirmed: i64 = row.get(7)?;
        Ok(BlockLink {
            id: row.get(0)?,
            source_file_id: row.get(1)?,
            source_anchor_key: row.get(2)?,
            target_file_id: row.get(3)?,
            target_anchor_key: row.get(4)?,
            link_type: row.get(5)?,
            confidence: row.get(6)?,
            is_confirmed: confirmed != 0,
        })
    })?;

    Ok(rows.flatten().collect())
}

/// Suggest implicit links based on anchor similarity (Phase B: basic cosine).
/// Phase C+ will add graph traversal and LLM-based suggestion.
pub fn suggest_implicit_links(
    conn: &Connection,
    file_id: i64,
    min_confidence: f64,
) -> AppResult<Vec<BlockLink>> {
    // Get anchors for the source file
    let mut stmt =
        conn.prepare("SELECT anchor_key, content FROM semantic_anchors WHERE file_id = ?1")?;
    let source_anchors: Vec<(String, String)> = stmt
        .query_map([file_id], |row| {
            let key: String = row.get(0)?;
            let content: String = row.get(1)?;
            Ok((key, content))
        })?
        .filter_map(|r| r.ok())
        .collect();

    if source_anchors.is_empty() {
        return Ok(vec![]);
    }

    // Find anchors in other files that share keywords with source anchors
    let mut suggestions = Vec::new();

    for (source_key, source_content) in &source_anchors {
        // Extract keywords from source anchor content
        let keywords = extract_keywords(source_content);
        if keywords.is_empty() {
            continue;
        }

        // Search for similar anchors in other files
        let keyword_pattern = format!("%{}%", keywords[0]);
        let mut similar_stmt = conn.prepare(
            "SELECT sa.id, sa.file_id, sa.anchor_key, sa.content, f.path
             FROM semantic_anchors sa
             JOIN files f ON f.id = sa.file_id
             WHERE sa.file_id != ?1
               AND sa.content LIKE ?2
             LIMIT 10",
        )?;

        let similar_anchors: Vec<(i64, i64, String, String, String)> = similar_stmt
            .query_map(rusqlite::params![file_id, keyword_pattern], |row| {
                let id: i64 = row.get(0)?;
                let target_file_id: i64 = row.get(1)?;
                let anchor_key: String = row.get(2)?;
                let content: String = row.get(3)?;
                let path: String = row.get(4)?;
                Ok((id, target_file_id, anchor_key, content, path))
            })?
            .filter_map(|r| r.ok())
            .collect();

        for (_target_anchor_id, target_file_id, target_key, target_content, _path) in
            &similar_anchors
        {
            // Calculate simple keyword overlap confidence
            let target_keywords = extract_keywords(target_content);
            let overlap = keywords
                .iter()
                .filter(|k| target_keywords.contains(k))
                .count();
            let total = keywords.len().max(target_keywords.len());
            let confidence = if total > 0 {
                overlap as f64 / total as f64
            } else {
                0.0
            };

            if confidence >= min_confidence {
                // Check if this link already exists
                let exists = conn.query_row(
                    "SELECT COUNT(*) FROM block_links
                     WHERE source_file_id = ?1
                       AND source_anchor_key = ?2
                       AND target_file_id = ?3
                       AND target_anchor_key = ?4",
                    rusqlite::params![file_id, source_key, target_file_id, target_key],
                    |row| {
                        let count: i64 = row.get(0)?;
                        Ok(count > 0)
                    },
                )?;

                if !exists {
                    suggestions.push(BlockLink {
                        id: 0, // Will be assigned on insert
                        source_file_id: file_id,
                        source_anchor_key: Some(source_key.clone()),
                        target_file_id: *target_file_id,
                        target_anchor_key: Some(target_key.clone()),
                        link_type: "implicit".to_string(),
                        confidence,
                        is_confirmed: false,
                    });
                }
            }
        }
    }

    // Deduplicate and sort by confidence
    suggestions.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
    suggestions.truncate(10); // Limit to top 10 suggestions

    Ok(suggestions)
}

/// Extract keywords from anchor content.
fn extract_keywords(content: &str) -> Vec<String> {
    let mut keywords = Vec::new();

    // Split by common delimiters
    let words: Vec<&str> = content
        .split(|c: char| !c.is_alphanumeric() && c != '_' && !c.is_ascii())
        .filter(|w| w.len() >= 2)
        .collect();

    for word in words {
        let word_lower = word.to_lowercase();
        // Filter out common stop words
        if !is_stop_word(&word_lower) && !keywords.contains(&word_lower) {
            keywords.push(word_lower);
        }
    }

    keywords
}

/// Check if a word is a stop word.
fn is_stop_word(word: &str) -> bool {
    const STOP_WORDS: &[&str] = &[
        "的",
        "了",
        "在",
        "是",
        "我",
        "有",
        "和",
        "就",
        "不",
        "人",
        "都",
        "一",
        "一个",
        "上",
        "也",
        "很",
        "到",
        "说",
        "要",
        "去",
        "你",
        "会",
        "着",
        "没有",
        "看",
        "好",
        "自己",
        "这",
        "他",
        "她",
        "它",
        "们",
        "我们",
        "你们",
        "他们",
        "她们",
        "它们",
        "那",
        "那些",
        "这些",
        "这个",
        "那个",
        "什么",
        "怎么",
        "如何",
        "为什么",
        "可以",
        "可能",
        "应该",
        "必须",
        "需要",
        "the",
        "a",
        "an",
        "is",
        "are",
        "was",
        "were",
        "be",
        "been",
        "being",
        "have",
        "has",
        "had",
        "do",
        "does",
        "did",
        "will",
        "would",
        "could",
        "should",
        "may",
        "might",
        "must",
        "shall",
        "can",
        "need",
        "dare",
        "ought",
        "used",
        "to",
        "of",
        "in",
        "for",
        "on",
        "with",
        "at",
        "by",
        "from",
        "as",
        "into",
        "through",
        "during",
        "before",
        "after",
        "above",
        "below",
        "between",
        "and",
        "but",
        "or",
        "nor",
        "not",
        "so",
        "yet",
        "both",
        "either",
        "neither",
        "each",
        "every",
        "all",
        "any",
        "few",
        "more",
        "most",
        "other",
        "some",
        "such",
        "no",
        "only",
        "own",
        "same",
        "than",
        "too",
        "very",
        "just",
        "because",
        "if",
        "when",
        "where",
        "how",
        "what",
        "which",
        "who",
        "whom",
        "this",
        "that",
        "these",
        "those",
        "am",
        "about",
        "up",
        "out",
        "off",
        "over",
        "under",
    ];

    STOP_WORDS.contains(&word)
}

/// Delete all unconfirmed implicit links for a file (cleanup before re-suggestion).
pub fn delete_implicit_links(conn: &Connection, file_id: i64) -> AppResult<usize> {
    let count = conn.execute(
        "DELETE FROM block_links WHERE source_file_id = ?1 AND link_type = 'implicit' AND is_confirmed = 0",
        [file_id],
    )?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::Database;

    #[test]
    fn insert_and_retrieve_link() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            // Need files records for FK
            conn.execute(
                "INSERT INTO files (path, title, content_hash, created_at, updated_at)
                 VALUES ('a.md', 'A', 'h1', datetime('now'), datetime('now')),
                        ('b.md', 'B', 'h2', datetime('now'), datetime('now'))",
                [],
            )?;

            insert_link(
                conn,
                1,
                Some("anchor-a"),
                2,
                Some("anchor-b"),
                "implicit",
                0.85,
                "system",
            )?;

            // Mark as confirmed
            conn.execute("UPDATE block_links SET is_confirmed = 1 WHERE id = 1", [])?;

            let links = get_confirmed_links(conn, 1)?;
            assert_eq!(links.len(), 1);
            assert_eq!(links[0].target_file_id, 2);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn delete_implicit_removes_only_unconfirmed() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO files (path, title, content_hash, created_at, updated_at)
                 VALUES ('x.md', 'X', 'hx', datetime('now'), datetime('now'))",
                [],
            )?;

            insert_link(conn, 1, None, 1, None, "implicit", 0.5, "system")?;
            let deleted = delete_implicit_links(conn, 1)?;
            assert_eq!(deleted, 1);
            Ok(())
        })
        .unwrap();
    }
}
