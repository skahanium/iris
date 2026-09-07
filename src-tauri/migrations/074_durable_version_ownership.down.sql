-- Run transactionally. An old schema cannot represent known ownership, archive
-- identity, orphan history or a changed index identity. Refuse lossy rollback.
CREATE TEMP TABLE version_rollback_guard_074 (safe INTEGER CHECK (safe = 1));
INSERT INTO version_rollback_guard_074
SELECT CASE WHEN EXISTS (
    SELECT 1 FROM versions v LEFT JOIN files f ON f.id = v.file_id
    WHERE v.vault_path IS NOT NULL OR v.recycle_id IS NOT NULL
       OR f.id IS NULL OR v.note_path IS NULL OR v.note_path != f.path
) OR EXISTS (
    SELECT 1 FROM versions GROUP BY file_id, version_no HAVING COUNT(*) > 1
) THEN 0 ELSE 1 END;
DROP TABLE version_rollback_guard_074;
CREATE TABLE versions_rebuild_074 (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    version_no TEXT NOT NULL,
    label TEXT,
    content_hash TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    word_count INTEGER DEFAULT 0,
    is_finalized INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'manual',
    UNIQUE(file_id, version_no)
);
INSERT INTO versions_rebuild_074
SELECT id, file_id, version_no, label, content_hash, storage_path, word_count,
       is_finalized, created_at, kind FROM versions;
DROP TABLE versions;
ALTER TABLE versions_rebuild_074 RENAME TO versions;
CREATE INDEX idx_versions_file ON versions(file_id);
CREATE INDEX idx_versions_finalized ON versions(is_finalized);
CREATE INDEX idx_versions_created ON versions(created_at);
CREATE INDEX idx_versions_file_kind_created ON versions(file_id, kind, created_at DESC);
