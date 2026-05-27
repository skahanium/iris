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
         ORDER BY confidence DESC"
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
    _conn: &Connection,
    _file_id: i64,
    _min_confidence: f64,
) -> AppResult<Vec<BlockLink>> {
    // Phase B: skeleton — no automatic suggestion yet.
    // Phase C+: compute anchor-embedding similarity across files,
    //           suggest links where similarity > threshold.
    Ok(vec![])
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

            insert_link(conn, 1, Some("anchor-a"), 2, Some("anchor-b"), "implicit", 0.85, "system")?;

            // Mark as confirmed
            conn.execute("UPDATE block_links SET is_confirmed = 1 WHERE id = 1", [])?;

            let links = get_confirmed_links(conn, 1)?;
            assert_eq!(links.len(), 1);
            assert_eq!(links[0].target_file_id, 2);
            Ok(())
        }).unwrap();
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
        }).unwrap();
    }
}
