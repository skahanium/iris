use rusqlite::Connection;

use super::engine::{embed_text, f32_to_bytes};
use crate::error::AppResult;
use crate::storage::db;

/// Embed all chunks for a file and store in chunk_embeddings.
pub fn store_chunk_embeddings(conn: &Connection, file_id: i64) -> AppResult<()> {
    let mut stmt =
        conn.prepare("SELECT id, content FROM chunks WHERE file_id = ?1 ORDER BY chunk_index")?;
    let rows = stmt.query_map([file_id], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;

    for row in rows.flatten() {
        let (chunk_id, content) = row;
        let embedding = match embed_text(&content) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("chunk {chunk_id} embedding skipped: {e}");
                continue;
            }
        };
        let blob = f32_to_bytes(&embedding);
        conn.execute(
            "INSERT OR REPLACE INTO chunk_embeddings (chunk_id, embedding) VALUES (?1, ?2)",
            rusqlite::params![chunk_id, blob],
        )?;
        if db::vector_index_ready() {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO vec_chunks (rowid, embedding) VALUES (?1, ?2)",
                rusqlite::params![chunk_id, blob],
            );
        }
    }
    Ok(())
}
