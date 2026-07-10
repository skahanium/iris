-- Embedding generation v2 keeps the legacy 384-dimensional cache readable while
-- BGE-small-zh-v1.5 (512 dimensions) is rebuilt into a separate derived table.
CREATE TABLE IF NOT EXISTS embedding_generation_state (
    singleton           INTEGER PRIMARY KEY CHECK (singleton = 1),
    active_model_id     TEXT NOT NULL,
    target_model_id     TEXT NOT NULL,
    target_dimension    INTEGER NOT NULL CHECK (target_dimension > 0),
    phase               TEXT NOT NULL CHECK (phase IN ('legacy_ready', 'rebuilding', 'ready', 'failed')),
    indexed_items       INTEGER NOT NULL DEFAULT 0 CHECK (indexed_items >= 0),
    total_items         INTEGER NOT NULL DEFAULT 0 CHECK (total_items >= 0),
    last_error          TEXT,
    updated_at          TEXT NOT NULL
);

INSERT OR IGNORE INTO embedding_generation_state (
    singleton, active_model_id, target_model_id, target_dimension, phase,
    indexed_items, total_items, last_error, updated_at
) VALUES (
    1, 'fastembed/AllMiniLML6V2', 'Xenova/bge-small-zh-v1.5', 512, 'rebuilding',
    0, 0, NULL, datetime('now')
);

CREATE TABLE IF NOT EXISTS chunk_embeddings_v2 (
    chunk_id    INTEGER PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
    embedding   BLOB NOT NULL
);