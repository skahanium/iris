use rusqlite::Connection;

use crate::error::AppResult;

/// Extract image references from markdown content: `![alt](path)`.
pub fn extract_image_refs(content: &str) -> Vec<ImageRef> {
    let re = regex::Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").unwrap();
    re.captures_iter(content)
        .map(|cap| ImageRef {
            alt: cap.get(1).map(|m| m.as_str().trim().to_string()),
            path: cap
                .get(2)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default(),
        })
        .filter(|r| !r.path.is_empty())
        .collect()
}

/// A parsed image reference from markdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    pub alt: Option<String>,
    pub path: String,
}

/// Index image references for a source file.
/// Clears existing image refs and inserts the current set.
/// Gracefully skips if the image_refs table doesn't exist yet (pre-migration).
pub fn index_image_refs(conn: &Connection, source_id: i64, content: &str) -> AppResult<usize> {
    // Check if table exists; skip silently if migration hasn't run yet
    let table_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='image_refs'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !table_exists {
        return Ok(0);
    }

    conn.execute("DELETE FROM image_refs WHERE source_id = ?1", [source_id])?;

    let refs = extract_image_refs(content);
    let mut count = 0;

    for r in &refs {
        conn.execute(
            "INSERT OR REPLACE INTO image_refs (source_id, image_path, alt_text) VALUES (?1, ?2, ?3)",
            rusqlite::params![source_id, r.path, r.alt],
        )?;
        count += 1;
    }

    Ok(count)
}

/// Read a file's content from disk by absolute path, then return its image references.
/// Used during cascading rename to check if relative image paths need adjustment.
pub fn collect_image_refs_from_disk(absolute: &std::path::Path) -> AppResult<Vec<ImageRef>> {
    let content = std::fs::read_to_string(absolute)?;
    Ok(extract_image_refs(&content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::Database;

    #[test]
    fn extracts_simple_image() {
        let refs = extract_image_refs("![logo](assets/logo.png)");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].alt.as_deref(), Some("logo"));
        assert_eq!(refs[0].path, "assets/logo.png");
    }

    #[test]
    fn extracts_image_without_alt() {
        let refs = extract_image_refs("![](photo.jpg)");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].alt.as_deref(), Some(""));
        assert_eq!(refs[0].path, "photo.jpg");
    }

    #[test]
    fn extracts_multiple_images() {
        let refs = extract_image_refs("![a](1.png) and ![b](2.png)");
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn ignores_wikilinks() {
        let refs = extract_image_refs("[[page]] and ![img](img.png)");
        assert_eq!(refs.len(), 1);
    }

    #[test]
    fn ignores_empty_path() {
        let refs = extract_image_refs("![x]()");
        assert_eq!(refs.len(), 0);
    }

    #[test]
    fn index_image_refs_persists() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "CREATE TABLE image_refs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    source_id INTEGER NOT NULL,
                    image_path TEXT NOT NULL,
                    alt_text TEXT,
                    UNIQUE(source_id, image_path)
                )",
                [],
            )?;

            let count = index_image_refs(conn, 1, "![a](a.png) ![b](b.png)")?;
            assert_eq!(count, 2);

            let rows: i64 = conn.query_row(
                "SELECT COUNT(*) FROM image_refs WHERE source_id = 1",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(rows, 2);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn index_image_refs_clears_old_on_reindex() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "CREATE TABLE image_refs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    source_id INTEGER NOT NULL,
                    image_path TEXT NOT NULL,
                    alt_text TEXT,
                    UNIQUE(source_id, image_path)
                )",
                [],
            )?;

            index_image_refs(conn, 1, "![a](a.png)")?;
            let count = index_image_refs(conn, 1, "![b](b.png)")?;
            assert_eq!(count, 1);

            let rows: i64 = conn.query_row(
                "SELECT COUNT(*) FROM image_refs WHERE source_id = 1",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(rows, 1);

            let path: String = conn.query_row(
                "SELECT image_path FROM image_refs WHERE source_id = 1",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(path, "b.png");
            Ok(())
        })
        .unwrap();
    }
}
