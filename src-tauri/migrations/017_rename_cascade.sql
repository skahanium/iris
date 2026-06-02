CREATE TABLE IF NOT EXISTS image_refs (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id       INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    image_path      TEXT NOT NULL,
    alt_text        TEXT,
    UNIQUE(source_id, image_path)
);

CREATE INDEX IF NOT EXISTS idx_image_refs_source ON image_refs(source_id);
CREATE INDEX IF NOT EXISTS idx_image_refs_path ON image_refs(image_path);
