use rusqlite::Connection;

use super::engine::{embed_text, f32_to_bytes};
use crate::error::AppResult;

/// Store one BGE v2 embedding without mutating the legacy 384-dimensional cache.
pub(crate) fn store_chunk_embedding_v2(
    conn: &Connection,
    chunk_id: i64,
    embedding: &[f32],
) -> AppResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO chunk_embeddings_v2 (chunk_id, embedding) VALUES (?1, ?2)",
        rusqlite::params![chunk_id, f32_to_bytes(embedding)],
    )?;
    Ok(())
}
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
        store_chunk_embedding_v2(conn, chunk_id, &embedding)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::store_chunk_embedding_v2;
    use crate::storage::migrate::migrate_up;
    use rusqlite::Connection;

    #[test]
    fn v2_writes_do_not_overwrite_legacy_embedding_rows() {
        let conn = Connection::open_in_memory().expect("open database");
        migrate_up(&conn).expect("migrate database");
        conn.execute(
            "INSERT INTO files (path, title, content_hash, word_count, created_at, updated_at)
             VALUES ('note.md', 'Note', 'hash', 1, datetime('now'), datetime('now'))",
            [],
        )
        .expect("insert file");
        conn.execute(
            "INSERT INTO chunks (file_id, chunk_index, content) VALUES (1, 0, 'body')",
            [],
        )
        .expect("insert chunk");
        conn.execute(
            "INSERT INTO chunk_embeddings (chunk_id, embedding) VALUES (1, x'01020304')",
            [],
        )
        .expect("insert legacy embedding");

        store_chunk_embedding_v2(&conn, 1, &[0.25_f32, -0.5_f32]).expect("store v2 embedding");

        let legacy: Vec<u8> = conn
            .query_row(
                "SELECT embedding FROM chunk_embeddings WHERE chunk_id = 1",
                [],
                |row| row.get(0),
            )
            .expect("read legacy embedding");
        let v2: Vec<u8> = conn
            .query_row(
                "SELECT embedding FROM chunk_embeddings_v2 WHERE chunk_id = 1",
                [],
                |row| row.get(0),
            )
            .expect("read v2 embedding");
        assert_eq!(legacy, vec![1, 2, 3, 4]);
        assert_eq!(v2.len(), 8);
    }
}
