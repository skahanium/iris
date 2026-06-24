-- 038: Attachment reference index for Markdown-to-media relationships.
CREATE TABLE IF NOT EXISTS attachment_refs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    source_path TEXT NOT NULL,
    target_path TEXT NOT NULL,
    ref_kind    TEXT NOT NULL DEFAULT 'embed',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    UNIQUE(source_path, target_path, ref_kind)
);

CREATE INDEX IF NOT EXISTS idx_attachment_refs_source
    ON attachment_refs(source_path);
CREATE INDEX IF NOT EXISTS idx_attachment_refs_target
    ON attachment_refs(target_path);
