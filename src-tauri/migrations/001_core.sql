-- files index
CREATE TABLE IF NOT EXISTS files (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    path        TEXT NOT NULL UNIQUE,
    title       TEXT,
    frontmatter TEXT,
    content_hash TEXT NOT NULL,
    word_count  INTEGER DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tags (
    id    INTEGER PRIMARY KEY AUTOINCREMENT,
    name  TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS file_tags (
    file_id INTEGER REFERENCES files(id) ON DELETE CASCADE,
    tag_id  INTEGER REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (file_id, tag_id)
);

CREATE TABLE IF NOT EXISTS links (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id  INTEGER REFERENCES files(id) ON DELETE CASCADE,
    target_id  INTEGER REFERENCES files(id) ON DELETE CASCADE,
    context    TEXT,
    UNIQUE(source_id, target_id)
);

CREATE TABLE IF NOT EXISTS chunks (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id      INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    chunk_index  INTEGER NOT NULL,
    content      TEXT NOT NULL,
    token_count  INTEGER,
    UNIQUE(file_id, chunk_index)
);

CREATE TABLE IF NOT EXISTS chunk_embeddings (
    chunk_id   INTEGER PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
    embedding  BLOB NOT NULL
);

CREATE VIRTUAL TABLE IF NOT EXISTS files_fts USING fts5(
    path,
    title,
    content,
    tokenize='unicode61'
);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_files_updated ON files(updated_at);
CREATE INDEX IF NOT EXISTS idx_chunks_file ON chunks(file_id);
