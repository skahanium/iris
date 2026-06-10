-- 025: Ensure scalar knowledge tables exist without requiring sqlite-vec.
-- 010 remains the historical sqlite-vec migration; this backfill creates the
-- ordinary SQLite tables and indexes that the default build can support.

CREATE TABLE IF NOT EXISTS semantic_anchors (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    anchor_key        TEXT NOT NULL UNIQUE,
    file_id           INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    anchor_type       TEXT NOT NULL,
    content           TEXT NOT NULL,
    heading_path      TEXT,
    source_start      INTEGER NOT NULL,
    source_end        INTEGER NOT NULL,
    paragraph_index   INTEGER,
    content_hash      TEXT NOT NULL,
    extractor_version TEXT NOT NULL,
    embedding_model   TEXT NOT NULL,
    embedding_dim     INTEGER NOT NULL,
    confidence        REAL NOT NULL,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS regulation_index (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id            INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    regulation_name    TEXT NOT NULL,
    issuer             TEXT,
    version_label      TEXT,
    chapter            TEXT,
    section            TEXT,
    article            TEXT NOT NULL,
    paragraph          TEXT,
    content            TEXT NOT NULL,
    keywords           TEXT,
    source_start       INTEGER NOT NULL,
    source_end         INTEGER NOT NULL,
    content_hash       TEXT NOT NULL,
    parser_version     TEXT NOT NULL,
    embedding_model    TEXT NOT NULL,
    embedding_dim      INTEGER NOT NULL,
    created_at         TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS genre_templates (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    template_key        TEXT NOT NULL UNIQUE,
    genre               TEXT NOT NULL,
    subtype             TEXT,
    structure           JSON NOT NULL,
    common_phrases      JSON,
    style_features      JSON,
    source_file_id      INTEGER REFERENCES files(id) ON DELETE SET NULL,
    source_content_hash TEXT,
    extractor_version   TEXT NOT NULL,
    user_confirmed      INTEGER NOT NULL DEFAULT 0,
    usage_count         INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS block_links (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    source_file_id     INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    source_anchor_key  TEXT,
    target_file_id     INTEGER REFERENCES files(id) ON DELETE CASCADE,
    target_anchor_key  TEXT,
    link_type          TEXT NOT NULL,
    confidence         REAL NOT NULL DEFAULT 1.0,
    is_confirmed       INTEGER NOT NULL DEFAULT 0,
    created_by         TEXT NOT NULL,
    context_hash       TEXT,
    created_at         TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_anchors_file ON semantic_anchors(file_id);
CREATE INDEX IF NOT EXISTS idx_anchors_type ON semantic_anchors(anchor_type);
CREATE INDEX IF NOT EXISTS idx_regulation_file ON regulation_index(file_id);
CREATE INDEX IF NOT EXISTS idx_regulation_name ON regulation_index(regulation_name);
CREATE INDEX IF NOT EXISTS idx_regulation_article ON regulation_index(regulation_name, article);
CREATE INDEX IF NOT EXISTS idx_block_links_source ON block_links(source_file_id);
CREATE INDEX IF NOT EXISTS idx_block_links_target ON block_links(target_file_id);
CREATE INDEX IF NOT EXISTS idx_block_links_type ON block_links(link_type);
CREATE INDEX IF NOT EXISTS idx_templates_genre ON genre_templates(genre);
