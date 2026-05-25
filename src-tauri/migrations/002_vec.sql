-- v0.2: sqlite-vec virtual table for approximate nearest-neighbor search.
-- Replaces the v0.1 approach of loading all chunk_embeddings BLOBs into Rust
-- and computing cosine similarity in application code.
--
-- Requirements: sqlite-vec extension must be loaded (crate: sqlite-vec).
-- If unavailable, v0.1 cosine fallback path remains active.

CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(
    embedding float[384]
);

-- Backfill from chunk_embeddings BLOB table into vec_chunks.
-- Each row in chunk_embeddings has (chunk_id, embedding BLOB).
-- We store the chunk_id as the rowid in vec_chunks for JOIN compatibility.
-- NOTE: This backfill iterates all existing embeddings and may be slow
-- for large vaults. Consider running as a background task after migration.

INSERT OR IGNORE INTO vec_chunks (rowid, embedding)
SELECT ce.chunk_id, ce.embedding
FROM chunk_embeddings ce
WHERE NOT EXISTS (
    SELECT 1 FROM vec_chunks vc WHERE vc.rowid = ce.chunk_id
);
