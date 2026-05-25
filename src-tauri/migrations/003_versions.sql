CREATE TABLE IF NOT EXISTS versions (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id      INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    version_no   TEXT NOT NULL,
    label        TEXT,
    content_hash TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    word_count   INTEGER DEFAULT 0,
    is_finalized INTEGER DEFAULT 0,
    created_at   TEXT NOT NULL,
    UNIQUE(file_id, version_no)
);

CREATE INDEX IF NOT EXISTS idx_versions_file ON versions(file_id);
CREATE INDEX IF NOT EXISTS idx_versions_finalized ON versions(is_finalized);
CREATE INDEX IF NOT EXISTS idx_versions_created ON versions(created_at);
