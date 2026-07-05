DROP INDEX IF EXISTS idx_chunks_content_hash;
ALTER TABLE chunks DROP COLUMN content_hash;
ALTER TABLE chunks DROP COLUMN source_end;
ALTER TABLE chunks DROP COLUMN source_start;
ALTER TABLE chunks DROP COLUMN heading_path;
