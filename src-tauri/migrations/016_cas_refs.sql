CREATE TABLE IF NOT EXISTS cas_refs (
    object_hash     TEXT PRIMARY KEY,
    ref_count       INTEGER NOT NULL DEFAULT 0,
    object_type     TEXT NOT NULL DEFAULT 'unknown',
    size            INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL,
    last_accessed_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS cas_ref_links (
    source_hash TEXT NOT NULL,
    target_hash TEXT NOT NULL,
    PRIMARY KEY (source_hash, target_hash)
);

CREATE INDEX IF NOT EXISTS idx_cas_refs_ref_count ON cas_refs(ref_count);
CREATE INDEX IF NOT EXISTS idx_cas_refs_object_type ON cas_refs(object_type);
CREATE INDEX IF NOT EXISTS idx_cas_ref_links_source ON cas_ref_links(source_hash);
CREATE INDEX IF NOT EXISTS idx_cas_ref_links_target ON cas_ref_links(target_hash);

-- chunks 表新增 cas_hash 字段
ALTER TABLE chunks ADD COLUMN cas_hash TEXT;
