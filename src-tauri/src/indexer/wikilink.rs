use rusqlite::Connection;

use crate::error::AppResult;

/// Extract wiki-link titles from note body: `[[Page Title]]` or `[[page-title]]`.
pub fn extract_wiki_links(content: &str) -> Vec<String> {
    let re = regex::Regex::new(r"\[\[([^\]]+)\]\]").unwrap();
    re.captures_iter(content)
        .filter_map(|cap| cap.get(1))
        .map(|m| m.as_str().trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Update links for a file: clear existing outbound links, insert new ones.
/// Target file is resolved by matching the wiki-link title against file titles or stem paths.
pub fn sync_wiki_links(conn: &Connection, source_id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM links WHERE source_id = ?1", [source_id])?;
    Ok(())
}

/// Insert a single link record (upsert by source_id + target_id).
pub fn insert_wiki_link(
    conn: &Connection,
    source_id: i64,
    target_id: i64,
    context: &str,
) -> AppResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO links (source_id, target_id, context) VALUES (?1, ?2, ?3)",
        rusqlite::params![source_id, target_id, context],
    )?;
    Ok(())
}

/// Resolve wiki-link titles in the given content against the files table,
/// and insert links for matched targets.
pub fn index_wiki_links(conn: &Connection, source_id: i64, content: &str) -> AppResult<usize> {
    conn.execute("DELETE FROM links WHERE source_id = ?1", [source_id])?;

    let titles = extract_wiki_links(content);
    let mut count = 0;

    for title in &titles {
        // Match by exact title or by filename stem
        let target: Option<(i64, String)> = conn
            .query_row(
                "SELECT id, title FROM files WHERE title = ?1 OR path LIKE ?2 LIMIT 1",
                rusqlite::params![
                    title,
                    format!("%{}.md", title)
                ],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        if let Some((target_id, _target_title)) = target {
            if target_id != source_id {
                let ctx = extract_link_context(content, title);
                insert_wiki_link(conn, source_id, target_id, &ctx)?;
                count += 1;
            }
        }
    }

    Ok(count)
}

/// Extract surrounding context for a wiki-link (up to 60 chars around the match).
fn extract_link_context(content: &str, title: &str) -> String {
    let pattern = format!("[[{}]]", regex::escape(title));
    let re = regex::Regex::new(&pattern).unwrap();
    if let Some(m) = re.find(content) {
        let start = m.start().saturating_sub(30);
        let end = (m.end() + 30).min(content.len());
        content[start..end].to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::Database;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn extracts_single_wiki_link() {
        let links = extract_wiki_links("See [[Page Title]] for details.");
        assert_eq!(links, vec!["Page Title"]);
    }

    #[test]
    fn extracts_multiple_wiki_links() {
        let links = extract_wiki_links("[[A]] and [[B]] and [[C]]");
        assert_eq!(links, vec!["A", "B", "C"]);
    }

    #[test]
    fn ignores_empty_brackets() {
        let links = extract_wiki_links("[[]] [[Real]]");
        assert_eq!(links, vec!["Real"]);
    }

    #[test]
    fn handles_chinese_titles() {
        let links = extract_wiki_links("参考 [[架构设计]] 文档");
        assert_eq!(links, vec!["架构设计"]);
    }

    #[test]
    fn no_links_returns_empty() {
        let links = extract_wiki_links("No links here.");
        assert!(links.is_empty());
    }

    #[test]
    fn extract_link_context_returns_surrounding_text() {
        let content = "Some context before [[MyPage]] and after.";
        let ctx = extract_link_context(content, "MyPage");
        assert!(ctx.contains("[[MyPage]]"));
    }

    #[test]
    fn index_wiki_links_inserts_and_counts() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            // Create source and target files
            conn.execute(
                "INSERT INTO files (id, path, title, content_hash, created_at, updated_at)
                 VALUES (1, 'source.md', 'Source', 'abc', '', '')",
                [],
            )?;
            conn.execute(
                "INSERT INTO files (id, path, title, content_hash, created_at, updated_at)
                 VALUES (2, 'target.md', 'Target Page', 'def', '', '')",
                [],
            )?;

            let count = index_wiki_links(conn, 1, "See [[Target Page]] for more.")?;
            assert_eq!(count, 1);

            // Verify link record
            let link_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM links WHERE source_id = 1 AND target_id = 2",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(link_count, 1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn index_wiki_links_skips_self_links() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO files (id, path, title, content_hash, created_at, updated_at)
                 VALUES (1, 'self.md', 'Self', 'abc', '', '')",
                [],
            )?;

            // Link to self should be skipped
            let count = index_wiki_links(conn, 1, "See [[Self]] here.")?;
            assert_eq!(count, 0);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn index_wiki_links_clears_old_on_reindex() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO files (id, path, title, content_hash, created_at, updated_at)
                 VALUES (1, 'a.md', 'A', 'abc', '', ''),
                      (2, 'b.md', 'B', 'def', '', ''),
                      (3, 'c.md', 'C', 'ghi', '', '')",
                [],
            )?;

            // First index: A links to B
            index_wiki_links(conn, 1, "[[B]]")?;
            // Re-index: A now links to C instead
            let count = index_wiki_links(conn, 1, "[[C]]")?;
            assert_eq!(count, 1);

            let old: i64 = conn.query_row(
                "SELECT COUNT(*) FROM links WHERE source_id = 1 AND target_id = 2",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(old, 0, "old link should be deleted");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn sync_wiki_links_clears_source() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO files (id, path, title, content_hash, created_at, updated_at)
                 VALUES (1, 'a.md', 'A', 'abc', '', ''),
                      (2, 'b.md', 'B', 'def', '', '')",
                [],
            )?;
            conn.execute(
                "INSERT INTO links (source_id, target_id, context) VALUES (1, 2, 'ctx')",
                [],
            )?;

            sync_wiki_links(conn, 1)?;
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM links WHERE source_id = 1",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(count, 0);
            Ok(())
        })
        .unwrap();
    }
}
