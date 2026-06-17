use rusqlite::Connection;
use std::collections::BTreeSet;
use std::sync::LazyLock;

use crate::error::AppResult;

static WIKI_LINK_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\[\[([^\]]+)\]\]").expect("wiki-link regex"));

/// Extract wiki-link titles from note body: `[[Page Title]]` or `[[page-title]]`.
/// Skips matches inside fenced code blocks, inline code, and HTML comments.
pub fn extract_wiki_links(content: &str) -> Vec<String> {
    let mut fence = crate::indexer::code_fence::FenceState::new();
    let mut links = Vec::new();

    for line in content.lines() {
        let in_fence = fence.feed(line);
        if in_fence {
            continue;
        }
        for cap in WIKI_LINK_RE.captures_iter(line) {
            if let Some(m) = cap.get(0) {
                if crate::indexer::code_fence::FenceState::is_inside_inline_code_or_comment(
                    line,
                    m.start(),
                ) {
                    continue;
                }
            }
            if let Some(title) = cap.get(1) {
                let t = title.as_str().trim().to_string();
                if !t.is_empty() {
                    links.push(t);
                }
            }
        }
    }
    links
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

    let titles: BTreeSet<String> = extract_wiki_links(content).into_iter().collect();
    let mut count = 0;
    if titles.is_empty() {
        return Ok(0);
    }

    let files = load_link_targets(conn)?;

    for title in &titles {
        if let Some((target_id, _target_title, _target_path)) = resolve_wiki_target(&files, title) {
            if *target_id != source_id {
                let ctx = extract_link_context(content, title);
                insert_wiki_link(conn, source_id, *target_id, &ctx)?;
                count += 1;
            }
        }
    }

    Ok(count)
}

fn load_link_targets(conn: &Connection) -> AppResult<Vec<(i64, String, String)>> {
    let mut stmt = conn.prepare("SELECT id, title, path FROM files ORDER BY id")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    Ok(rows.flatten().collect())
}

fn resolve_wiki_target<'a>(
    files: &'a [(i64, String, String)],
    title: &str,
) -> Option<&'a (i64, String, String)> {
    files.iter().find(|(_, target_title, path)| {
        target_title == title || path_stem_matches_wiki_title(path, title)
    })
}

fn path_stem_matches_wiki_title(path: &str, title: &str) -> bool {
    let file_name = path
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path);
    if file_name.len() <= 3 || !file_name[file_name.len() - 3..].eq_ignore_ascii_case(".md") {
        return false;
    }
    file_name[..file_name.len() - 3].eq_ignore_ascii_case(title)
}

/// Extract surrounding context for a wiki-link (up to 60 chars around the match).
fn extract_link_context(content: &str, title: &str) -> String {
    let pattern = format!("[[{title}]]");
    if let Some(start_match) = content.find(&pattern) {
        let end_match = start_match + pattern.len();
        let start = start_match.saturating_sub(30);
        let end = (end_match + 30).min(content.len());
        content[start..end].to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::Database;

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
    fn index_wiki_links_matches_path_stem_case_insensitively() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO files (id, path, title, content_hash, created_at, updated_at)
                 VALUES (1, 'source.md', 'Source', 'abc', '', ''),
                        (2, 'notes/a.md', 'a', 'def', '', '')",
                [],
            )?;

            let count = index_wiki_links(conn, 1, "See [[A]] for more.")?;
            assert_eq!(count, 1);

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
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM links WHERE source_id = 1", [], |r| {
                    r.get(0)
                })?;
            assert_eq!(count, 0);
            Ok(())
        })
        .unwrap();
    }
}
