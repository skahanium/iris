use std::fs;
use std::path::Path;

use chrono::Utc;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use super::chunker::chunk_markdown;
use super::frontmatter::parse_note;
use super::fts::{delete_fts, upsert_fts};
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

    upsert_fts(conn, &rel, &title, &parsed.body)?;

    let chunks = chunk_markdown(&parsed.body, 2000);
    for (idx, chunk) in chunks.iter().enumerate() {
        conn.execute(
            "INSERT INTO chunks (file_id, chunk_index, content, token_count) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![file_id, idx as i64, chunk, chunk.len() as i64],
        )?;
    }

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
