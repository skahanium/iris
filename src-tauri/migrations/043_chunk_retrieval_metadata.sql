ALTER TABLE chunks ADD COLUMN heading_path TEXT;
ALTER TABLE chunks ADD COLUMN source_start INTEGER;
ALTER TABLE chunks ADD COLUMN source_end INTEGER;
ALTER TABLE chunks ADD COLUMN content_hash TEXT;

CREATE INDEX IF NOT EXISTS idx_chunks_content_hash
    ON chunks(content_hash);
