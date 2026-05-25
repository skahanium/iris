use std::fs;
use std::path::Path;

use chrono::Utc;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use super::chunker::chunk_markdown;
use super::frontmatter::parse_note;
use super::fts::{delete_fts, upsert_fts};
use super::wikilink::index_wiki_links;
#[cfg(not(test))]
use crate::embedding::store::store_chunk_embeddings;
use crate::error::AppResult;
use crate::storage::paths::relative_path;

#[derive(Debug, Clone, serde::Serialize)]
pub struct FileEntry {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub updated_at: String,
    pub word_count: i64,
}

fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

fn parse_title(path: &str, content: &str) -> String {
    if let Some(line) = content.lines().find(|l| l.starts_with("# ")) {
        return line.trim_start_matches("# ").trim().to_string();
    }
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}

fn word_count(content: &str) -> i64 {
    content.split_whitespace().count() as i64
}

/// 同步 `tags` / `file_tags`（先清空该文件的关联，再写入）。
pub fn sync_file_tags(conn: &Connection, file_id: i64, tags: &[String]) -> AppResult<()> {
    conn.execute("DELETE FROM file_tags WHERE file_id = ?1", [file_id])?;
    for tag in tags {
        let name = tag.trim();
        if name.is_empty() {
            continue;
        }
        conn.execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", [name])?;
        let tag_id: i64 = conn.query_row("SELECT id FROM tags WHERE name = ?1", [name], |row| {
            row.get(0)
        })?;
        conn.execute(
            "INSERT OR IGNORE INTO file_tags (file_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![file_id, tag_id],
        )?;
    }
    Ok(())
}

/// Index a single file into SQLite.
pub fn index_file(conn: &Connection, vault: &Path, absolute: &Path) -> AppResult<FileEntry> {
    let rel = relative_path(vault, absolute)?;
    let content = fs::read_to_string(absolute)?;
    let hash = content_hash(&content);
    let parsed = parse_note(&content)?;
    let title = parsed
        .title
        .clone()
        .unwrap_or_else(|| parse_title(&rel, &parsed.body));
    let wc = word_count(&parsed.body);
    let now = Utc::now().to_rfc3339();
    let frontmatter = parsed.frontmatter_json.as_deref();

    let existing_id: Option<i64> = conn
        .query_row("SELECT id FROM files WHERE path = ?1", [&rel], |row| {
            row.get(0)
        })
        .ok();

    let file_id = if let Some(id) = existing_id {
        conn.execute(
            "UPDATE files SET title = ?1, frontmatter = ?2, content_hash = ?3, word_count = ?4, updated_at = ?5 WHERE id = ?6",
            rusqlite::params![title, frontmatter, hash, wc, now, id],
        )?;
        conn.execute("DELETE FROM chunks WHERE file_id = ?1", [id])?;
        id
    } else {
        conn.execute(
            "INSERT INTO files (path, title, frontmatter, content_hash, word_count, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            rusqlite::params![rel, title, frontmatter, hash, wc, now],
        )?;
        conn.last_insert_rowid()
    };

    sync_file_tags(conn, file_id, &parsed.tags)?;

    let _link_count = index_wiki_links(conn, file_id, &parsed.body)?;

    upsert_fts(conn, &rel, &title, &parsed.body)?;

    let chunks = chunk_markdown(&parsed.body, 2000);
    for (idx, chunk) in chunks.iter().enumerate() {
        conn.execute(
            "INSERT INTO chunks (file_id, chunk_index, content, token_count) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![file_id, idx as i64, chunk, chunk.len() as i64],
        )?;
    }

    #[cfg(not(test))]
    store_chunk_embeddings(conn, file_id)?;

    Ok(FileEntry {
        id: file_id,
        path: rel,
        title,
        updated_at: now,
        word_count: wc,
    })
}

/// Remove file from index.
pub fn remove_file_index(conn: &Connection, path: &str) -> AppResult<()> {
    delete_fts(conn, path)?;
    conn.execute("DELETE FROM files WHERE path = ?1", [path])?;
    Ok(())
}

/// Recursively scan vault for `.md` files.
pub fn scan_vault(conn: &Connection, vault: &Path) -> AppResult<Vec<FileEntry>> {
    let mut entries = Vec::new();
    if !vault.exists() {
        return Ok(entries);
    }

    for entry in WalkDir::new(vault)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
    {
        let path = entry.path();
        if path.is_file() {
            entries.push(index_file(conn, vault, path)?);
        }
    }
    Ok(entries)
}

/// Compute SHA-256 hash for external change detection.
pub fn file_hash(path: &Path) -> AppResult<String> {
    let content = fs::read_to_string(path)?;
    Ok(content_hash(&content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::Database;
    use std::fs;
    use tempfile::tempdir;

    fn setup_vault() -> (tempfile::TempDir, std::path::PathBuf, Database) {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let db = Database::open_in_memory().unwrap();
        (dir, vault, db)
    }

    fn write_note(vault: &std::path::Path, rel: &str, content: &str) {
        let abs = vault.join(rel);
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&abs, content).unwrap();
    }

    #[test]
    fn index_file_creates_files_and_chunks() {
        let (_dir, vault, db) = setup_vault();
        write_note(&vault, "hello.md", "# Hello\n\nWorld.");

        db.with_conn(|conn| {
            let entry = index_file(conn, &vault, &vault.join("hello.md"))?;
            assert_eq!(entry.path, "hello.md");
            assert_eq!(entry.title, "Hello");

            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM files WHERE path = 'hello.md'",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(count, 1);

            let chunk_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM chunks WHERE file_id = ?1",
                [entry.id],
                |r| r.get(0),
            )?;
            assert!(chunk_count > 0, "should have at least one chunk");

            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn index_file_updates_existing() {
        let (_dir, vault, db) = setup_vault();
        write_note(&vault, "note.md", "# First");

        db.with_conn(|conn| {
            let e1 = index_file(conn, &vault, &vault.join("note.md"))?;

            // Rewrite file on disk
            fs::write(vault.join("note.md"), "# Second\n\nMore content.").unwrap();
            let e2 = index_file(conn, &vault, &vault.join("note.md"))?;

            assert_eq!(e1.id, e2.id, "same path should UPDATE not INSERT");
            assert_eq!(e2.title, "Second");

            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM files WHERE path = 'note.md'",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(count, 1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn index_file_syncs_tags() {
        let (_dir, vault, db) = setup_vault();
        write_note(
            &vault,
            "tagged.md",
            "---\ntags: [rust, tauri]\n---\n# Tagged",
        );

        db.with_conn(|conn| {
            let entry = index_file(conn, &vault, &vault.join("tagged.md"))?;
            let tags: Vec<String> = conn
                .prepare(
                    "SELECT t.name FROM tags t
                     JOIN file_tags ft ON ft.tag_id = t.id
                     WHERE ft.file_id = ?1
                     ORDER BY t.name",
                )
                .unwrap()
                .query_map([entry.id], |r| r.get(0))
                .unwrap()
                .flatten()
                .collect();
            assert_eq!(tags, vec!["rust", "tauri"]);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn index_file_fts_searchable() {
        let (_dir, vault, db) = setup_vault();
        write_note(&vault, "searchable.md", "# FTS Test\n\npineapple");

        db.with_conn(|conn| {
            index_file(conn, &vault, &vault.join("searchable.md"))?;
            let hits: Vec<String> = conn
                .prepare("SELECT path FROM files_fts WHERE files_fts MATCH ?1")
                .unwrap()
                .query_map(["pineapple"], |r| r.get(0))
                .unwrap()
                .flatten()
                .collect();
            assert!(
                hits.contains(&"searchable.md".into()),
                "FTS should find pineapple in searchable.md"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn remove_file_index_cleans_up() {
        let (_dir, vault, db) = setup_vault();
        write_note(&vault, "del.md", "# To Delete");

        db.with_conn(|conn| {
            let entry = index_file(conn, &vault, &vault.join("del.md"))?;
            remove_file_index(conn, "del.md")?;

            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM files WHERE id = ?1",
                [entry.id],
                |r| r.get(0),
            )?;
            assert_eq!(count, 0);

            let fts: Vec<String> = conn
                .prepare("SELECT path FROM files_fts WHERE path = 'del.md'")
                .unwrap()
                .query_map([], |r| r.get(0))
                .unwrap()
                .flatten()
                .collect();
            assert!(fts.is_empty());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn scan_vault_filters_md_only() {
        let (_dir, vault, db) = setup_vault();
        write_note(&vault, "a.md", "# A");
        write_note(&vault, "b.txt", "not a note");
        write_note(&vault, "sub/c.md", "# C");

        db.with_conn(|conn| {
            let entries = scan_vault(conn, &vault)?;
            let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
            assert!(paths.contains(&"a.md"));
            assert!(paths.contains(&"sub/c.md"));
            assert!(!paths.contains(&"b.txt"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn scan_vault_empty_dir() {
        let (_dir, vault, db) = setup_vault();
        db.with_conn(|conn| {
            let entries = scan_vault(conn, &vault)?;
            assert!(entries.is_empty());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn index_file_extracts_wiki_links() {
        let (_dir, vault, db) = setup_vault();
        write_note(&vault, "a.md", "# Note A");
        write_note(&vault, "b.md", "# Note B\n\nSee [[Note A]] for context.");

        db.with_conn(|conn| {
            let _entry_a = index_file(conn, &vault, &vault.join("a.md"))?;
            let entry_b = index_file(conn, &vault, &vault.join("b.md"))?;

            // Verify link from B → A exists
            let link_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM links WHERE source_id = ?1",
                [entry_b.id],
                |r| r.get(0),
            )?;
            assert_eq!(link_count, 1, "should have one outbound link from B");

            let context: String = conn.query_row(
                "SELECT context FROM links WHERE source_id = ?1",
                [entry_b.id],
                |r| r.get(0),
            )?;
            assert!(context.contains("[[Note A]]"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn index_file_clears_links_on_reindex() {
        let (_dir, vault, db) = setup_vault();
        write_note(&vault, "a.md", "# A");
        write_note(&vault, "b.md", "# B\n\n[[A]]");

        db.with_conn(|conn| {
            let _a = index_file(conn, &vault, &vault.join("a.md"))?;
            let b = index_file(conn, &vault, &vault.join("b.md"))?;

            let count1: i64 = conn.query_row(
                "SELECT COUNT(*) FROM links WHERE source_id = ?1",
                [b.id],
                |r| r.get(0),
            )?;
            assert_eq!(count1, 1);

            // Rewrite B without wikilinks
            fs::write(vault.join("b.md"), "# B\n\nNo links anymore.").unwrap();
            index_file(conn, &vault, &vault.join("b.md"))?;

            let count2: i64 = conn.query_row(
                "SELECT COUNT(*) FROM links WHERE source_id = ?1",
                [b.id],
                |r| r.get(0),
            )?;
            assert_eq!(count2, 0, "old links should be cleared on reindex");
            Ok(())
        })
        .unwrap();
    }
}
