use rusqlite::Connection;

use crate::error::AppResult;

/// Upsert a row into FTS5 shadow content.
pub fn upsert_fts(conn: &Connection, path: &str, title: &str, content: &str) -> AppResult<()> {
    conn.execute("DELETE FROM files_fts WHERE path = ?1", [path])?;
    conn.execute(
        "INSERT INTO files_fts (path, title, content) VALUES (?1, ?2, ?3)",
        [path, title, content],
    )?;
    Ok(())
}

/// Remove FTS entry for a path.
pub fn delete_fts(conn: &Connection, path: &str) -> AppResult<()> {
    conn.execute("DELETE FROM files_fts WHERE path = ?1", [path])?;
    Ok(())
}
