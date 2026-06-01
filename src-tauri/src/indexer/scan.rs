use std::fs;
use std::path::Path;

use chrono::Utc;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use super::chunker::chunk_markdown;
use super::frontmatter::{parse_note, resolve_display_title};
use super::fts::{delete_fts, upsert_fts};
use super::wikilink::index_wiki_links;
use crate::app::AppState;
#[cfg(not(test))]
use crate::embedding::store::store_chunk_embeddings;
use crate::error::AppResult;
use crate::storage::paths::{is_user_note_path, relative_path, resolve_vault_path};
use std::sync::Arc;

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
    index_file_with_embed(conn, vault, absolute, None)
}

/// Index with optional background embedding queue (production paths should pass `Some`).
pub fn index_file_with_embed(
    conn: &Connection,
    vault: &Path,
    absolute: &Path,
    #[allow(unused_variables)] app: Option<&Arc<AppState>>,
) -> AppResult<FileEntry> {
    let rel = relative_path(vault, absolute)?;
    if !is_user_note_path(&rel) {
        return Err(crate::error::AppError::msg(
            "Path is not a user note (metadata paths are not indexed)",
        ));
    }
    let content = fs::read_to_string(absolute)?;
    let hash = content_hash(&content);
    let parsed = parse_note(&content)?;
    let document_name = Path::new(&rel)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&rel)
        .to_string();
    let wc = word_count(&parsed.body);
    let now = Utc::now().to_rfc3339();
    let frontmatter = parsed.frontmatter_json.as_deref();

    let display_title =
        resolve_display_title(parsed.title.as_deref(), "", frontmatter, &document_name);

    let existing_row: Option<(i64, String, String, i64)> = conn
        .query_row(
            "SELECT id, content_hash, title, word_count FROM files WHERE path = ?1",
            [&rel],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .ok();

    if let Some((id, stored_hash, title, stored_wc)) = &existing_row {
        if stored_hash == &hash {
            tracing::debug!(path = %rel, "index_file skipped: content unchanged");
            return Ok(FileEntry {
                id: *id,
                path: rel,
                title: title.clone(),
                updated_at: conn
                    .query_row("SELECT updated_at FROM files WHERE id = ?1", [id], |r| {
                        r.get(0)
                    })
                    .unwrap_or(now),
                word_count: *stored_wc,
            });
        }
    }

    let existing_id: Option<i64> = existing_row.as_ref().map(|(id, _, _, _)| *id);

    let file_id = if let Some(id) = existing_id {
        conn.execute(
            "UPDATE files SET title = ?1, frontmatter = ?2, content_hash = ?3, word_count = ?4, updated_at = ?5 WHERE id = ?6",
            rusqlite::params![display_title, frontmatter, hash, wc, now, id],
        )?;
        conn.execute("DELETE FROM chunks WHERE file_id = ?1", [id])?;
        id
    } else {
        conn.execute(
            "INSERT INTO files (path, title, frontmatter, content_hash, word_count, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            rusqlite::params![rel, display_title, frontmatter, hash, wc, now],
        )?;
        conn.last_insert_rowid()
    };

    let title: String =
        conn.query_row("SELECT title FROM files WHERE id = ?1", [file_id], |r| {
            r.get(0)
        })?;

    sync_file_tags(conn, file_id, &parsed.tags)?;

    let _link_count = index_wiki_links(conn, file_id, &parsed.body)?;

    upsert_fts(conn, &rel, &title, &parsed.body)?;

    let chunks = chunk_markdown(&parsed.body, 2000);
    for (idx, chunk) in chunks.iter().enumerate() {
        conn.execute(
            "INSERT INTO chunks (file_id, chunk_index, content, char_count) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![file_id, idx as i64, chunk, chunk.len() as i64],
        )?;
    }

    #[cfg(not(test))]
    match app {
        Some(state) => state.enqueue_embedding(file_id),
        None => {
            if let Err(e) = store_chunk_embeddings(conn, file_id) {
                tracing::warn!("embedding skipped for file {file_id}: {e}");
            }
        }
    }

    Ok(FileEntry {
        id: file_id,
        path: rel,
        title,
        updated_at: now,
        word_count: wc,
    })
}

/// Incrementally index vault files whose content hash differs from the DB.
pub fn index_vault_incremental(
    conn: &Connection,
    vault: &Path,
    app: Option<&Arc<AppState>>,
) -> AppResult<Vec<FileEntry>> {
    let files = collect_vault_files(vault);
    let mut entries = Vec::with_capacity(files.len());
    for abs in files {
        let rel = match relative_path(vault, &abs) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if !is_user_note_path(&rel) {
            continue;
        }
        let disk_hash = match file_hash(&abs) {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!("index skip {}: {e}", abs.display());
                continue;
            }
        };
        let unchanged: bool = conn
            .query_row(
                "SELECT 1 FROM files WHERE path = ?1 AND content_hash = ?2",
                rusqlite::params![rel, disk_hash],
                |_| Ok(()),
            )
            .is_ok();
        if unchanged {
            if let Ok(entry) = conn.query_row(
                "SELECT id, path, title, updated_at, word_count FROM files WHERE path = ?1",
                [&rel],
                |row| {
                    Ok(FileEntry {
                        id: row.get(0)?,
                        path: row.get(1)?,
                        title: row.get(2)?,
                        updated_at: row.get(3)?,
                        word_count: row.get(4)?,
                    })
                },
            ) {
                entries.push(entry);
            }
            continue;
        }
        match index_file_with_embed(conn, vault, &abs, app) {
            Ok(entry) => entries.push(entry),
            Err(e) => tracing::warn!("index failed for {}: {e}", abs.display()),
        }
    }
    let _ = prune_stale_file_indexes(conn, vault)?;
    Ok(entries)
}

/// Remove file from index.
pub fn remove_file_index(conn: &Connection, path: &str) -> AppResult<()> {
    delete_fts(conn, path)?;
    conn.execute("DELETE FROM files WHERE path = ?1", [path])?;
    Ok(())
}

/// Drop index rows for user notes whose `.md` files are missing on disk.
pub fn prune_stale_file_indexes(conn: &Connection, vault: &Path) -> AppResult<usize> {
    let mut stmt = conn.prepare("SELECT DISTINCT path FROM files WHERE path NOT LIKE '.iris/%'")?;
    let paths: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    let mut pruned = 0usize;
    for path in paths {
        let stale = match resolve_vault_path(vault, &path) {
            Ok(abs) => !abs.is_file(),
            Err(e) => {
                tracing::warn!(path = %path, error = %e, "prune: path outside vault or invalid");
                true
            }
        };
        if stale {
            remove_file_index(conn, &path)?;
            pruned += 1;
        }
    }
    Ok(pruned)
}

/// Collect vault subfolders (forward-slash paths with trailing `/`), excluding `.iris`.
pub fn collect_vault_folders(vault: &Path) -> Vec<String> {
    if !vault.exists() {
        return Vec::new();
    }
    let mut folders = Vec::new();
    for entry in WalkDir::new(vault)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            e.path()
                .strip_prefix(vault)
                .is_ok_and(|rel| !rel.components().any(|c| c.as_os_str() == ".iris"))
        })
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_dir() {
            continue;
        }
        let Ok(rel) = relative_path(vault, entry.path()) else {
            continue;
        };
        if rel.is_empty() {
            continue;
        }
        folders.push(format!("{rel}/"));
    }
    folders.sort();
    folders.dedup();
    folders
}

/// Recursively scan vault for `.md` files (full index; prefer `index_vault_incremental`).
pub fn scan_vault(conn: &Connection, vault: &Path) -> AppResult<Vec<FileEntry>> {
    index_vault_incremental(conn, vault, None::<&Arc<AppState>>)
}

/// Collect all `.md` file paths in the vault without holding a DB lock.
pub fn collect_vault_files(vault: &Path) -> Vec<std::path::PathBuf> {
    if !vault.exists() {
        return Vec::new();
    }
    WalkDir::new(vault)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            e.path()
                .strip_prefix(vault)
                .is_ok_and(|rel| !rel.components().any(|c| c.as_os_str() == ".iris"))
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .filter(|e| e.path().is_file())
        .map(|e| e.path().to_path_buf())
        .collect()
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

    #[test]
    fn scan_vault_skips_iris_version_snapshots() {
        let (_dir, vault, db) = setup_vault();
        write_note(&vault, "real.md", "# Real\n\nBody.");
        let snap_dir = vault.join(".iris/versions/1");
        fs::create_dir_all(&snap_dir).unwrap();
        fs::write(snap_dir.join("20260101120000.md"), "# Snapshot\n\nOld.").unwrap();

        db.with_conn(|conn| {
            let entries = scan_vault(conn, &vault)?;
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].path, "real.md");
            let count: i64 = conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?;
            assert_eq!(count, 1);
            Ok(())
        })
        .unwrap();
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
            assert_eq!(entry.title, "hello");

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
            fs::write(
                vault.join("note.md"),
                "---\ntitle: 第二版\n---\n\n# Second\n\nMore content.",
            )
            .unwrap();
            let e2 = index_file(conn, &vault, &vault.join("note.md"))?;

            assert_eq!(e1.id, e2.id, "same path should UPDATE not INSERT");
            assert_eq!(
                e2.title, "第二版",
                "title syncs from frontmatter on reindex"
            );

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
    fn index_file_skips_unchanged_content_hash() {
        let (_dir, vault, db) = setup_vault();
        write_note(&vault, "note.md", "# Stable\n\nBody.");

        db.with_conn(|conn| {
            index_file(conn, &vault, &vault.join("note.md"))?;
            let chunks_after_first: i64 =
                conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;
            index_file(conn, &vault, &vault.join("note.md"))?;
            let chunks_after_second: i64 =
                conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;
            assert_eq!(
                chunks_after_first, chunks_after_second,
                "unchanged file should not rebuild chunks"
            );
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
    fn prune_stale_file_indexes_removes_missing_files() {
        let (_dir, vault, db) = setup_vault();
        write_note(&vault, "live.md", "# Live");
        write_note(&vault, "gone.md", "# Gone");

        db.with_conn(|conn| {
            index_file(conn, &vault, &vault.join("live.md"))?;
            index_file(conn, &vault, &vault.join("gone.md"))?;
            fs::remove_file(vault.join("gone.md"))?;
            let pruned = prune_stale_file_indexes(conn, &vault)?;
            assert_eq!(pruned, 1);
            let paths: Vec<String> = conn
                .prepare("SELECT path FROM files")?
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            assert_eq!(paths, vec!["live.md".to_string()]);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn collect_vault_folders_includes_empty_dirs() {
        let (_dir, vault, _db) = setup_vault();
        fs::create_dir_all(vault.join("notes/inbox")).unwrap();
        let folders = collect_vault_folders(&vault);
        assert!(folders.contains(&"notes/".to_string()));
        assert!(folders.contains(&"notes/inbox/".to_string()));
    }

    #[test]
    fn prune_stale_file_indexes_drops_invalid_paths() {
        let (_dir, vault, db) = setup_vault();
        write_note(&vault, "live.md", "# Live");

        db.with_conn(|conn| {
            index_file(conn, &vault, &vault.join("live.md"))?;
            conn.execute(
                "INSERT INTO files (path, title, content_hash, word_count, created_at, updated_at)
                 VALUES ('../outside.md', 'x', 'h', 0, 'now', 'now')",
                [],
            )?;
            let pruned = prune_stale_file_indexes(conn, &vault)?;
            assert_eq!(pruned, 1);
            let paths: Vec<String> = conn
                .prepare("SELECT path FROM files")?
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            assert_eq!(paths, vec!["live.md".to_string()]);
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
        write_note(&vault, "b.md", "# Note B\n\nSee [[a]] for context.");

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
            assert!(context.contains("[[a]]"));
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
