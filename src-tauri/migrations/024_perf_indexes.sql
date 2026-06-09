-- P2-4: Performance indexes from audit recommendations
-- 1. Compound index for version policy queries (file_id + kind + created_at)
CREATE INDEX IF NOT EXISTS idx_versions_file_kind_created ON versions(file_id, kind, created_at);

-- 2. Index for chunk lookup by file_id + chunk_index (already unique, but explicit is clearer)
CREATE INDEX IF NOT EXISTS idx_chunks_file_index ON chunks(file_id, chunk_index);

-- 3. Index for semantic search cosine fallback filtering
CREATE INDEX IF NOT EXISTS idx_files_path_not_classified ON files(path) WHERE path NOT LIKE '.classified/%';
