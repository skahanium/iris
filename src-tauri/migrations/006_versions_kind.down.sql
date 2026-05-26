-- Rebuild versions without kind (SQLite-safe rollback).
CREATE TABLE versions_legacy (
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

INSERT INTO versions_legacy (
    id,
    file_id,
    version_no,
    label,
    content_hash,
    storage_path,
    word_count,
    is_finalized,
    created_at
)
SELECT
    id,
    file_id,
    version_no,
    label,
    content_hash,
    storage_path,
    word_count,
    is_finalized,
    created_at
FROM versions;

DROP TABLE versions;

ALTER TABLE versions_legacy RENAME TO versions;

CREATE INDEX IF NOT EXISTS idx_versions_file ON versions(file_id);
CREATE INDEX IF NOT EXISTS idx_versions_finalized ON versions(is_finalized);
CREATE INDEX IF NOT EXISTS idx_versions_created ON versions(created_at);
