-- History is application state, not a child of the disposable files index.
-- Historical vault ownership cannot be reconstructed from today's settings.
CREATE TABLE versions_rebuild_074 (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL,
    version_no TEXT NOT NULL,
    label TEXT,
    content_hash TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    word_count INTEGER DEFAULT 0,
    is_finalized INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'manual',
    vault_path TEXT,
    note_path TEXT,
    recycle_id TEXT,
    CHECK (vault_path IS NULL OR note_path IS NOT NULL),
    CHECK (recycle_id IS NULL OR vault_path IS NOT NULL)
);
INSERT INTO versions_rebuild_074
    (id, file_id, version_no, label, content_hash, storage_path, word_count,
     is_finalized, created_at, kind, note_path)
SELECT v.id, v.file_id, v.version_no, v.label, v.content_hash, v.storage_path,
       v.word_count, v.is_finalized, v.created_at, v.kind, f.path
FROM versions v LEFT JOIN files f ON f.id = v.file_id;
DROP TABLE versions;
ALTER TABLE versions_rebuild_074 RENAME TO versions;
CREATE INDEX idx_versions_file ON versions(file_id);
CREATE INDEX idx_versions_finalized ON versions(is_finalized);
CREATE INDEX idx_versions_created ON versions(created_at);
CREATE INDEX idx_versions_file_kind_created ON versions(file_id, kind, created_at DESC);
CREATE INDEX idx_versions_owner ON versions(vault_path, note_path, recycle_id, created_at DESC);
CREATE UNIQUE INDEX idx_versions_active_identity
    ON versions(vault_path, note_path, version_no)
    WHERE vault_path IS NOT NULL AND recycle_id IS NULL;
CREATE UNIQUE INDEX idx_versions_archived_identity
    ON versions(vault_path, recycle_id, version_no)
    WHERE recycle_id IS NOT NULL;
