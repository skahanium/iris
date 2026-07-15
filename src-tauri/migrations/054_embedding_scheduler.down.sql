CREATE TABLE embedding_generation_state_previous (
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
INSERT INTO embedding_generation_state_previous
SELECT singleton, active_model_id, target_model_id, target_dimension,
       CASE WHEN phase IN ('running', 'paused') THEN 'rebuilding' ELSE phase END,
       indexed_items, total_items, last_error, updated_at
FROM embedding_generation_state;
DROP TABLE embedding_generation_state;
ALTER TABLE embedding_generation_state_previous RENAME TO embedding_generation_state;

CREATE TABLE chunk_embeddings_v2_previous (
    chunk_id  INTEGER PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
    embedding BLOB NOT NULL
);
INSERT INTO chunk_embeddings_v2_previous SELECT chunk_id, embedding FROM chunk_embeddings_v2;
DROP TABLE chunk_embeddings_v2;
ALTER TABLE chunk_embeddings_v2_previous RENAME TO chunk_embeddings_v2;

CREATE TABLE semantic_anchor_embeddings_v2_previous (
    anchor_id INTEGER PRIMARY KEY REFERENCES semantic_anchors(id) ON DELETE CASCADE,
    embedding BLOB NOT NULL
);
INSERT INTO semantic_anchor_embeddings_v2_previous SELECT anchor_id, embedding FROM semantic_anchor_embeddings_v2;
DROP TABLE semantic_anchor_embeddings_v2;
ALTER TABLE semantic_anchor_embeddings_v2_previous RENAME TO semantic_anchor_embeddings_v2;

CREATE TABLE regulation_embeddings_v2_previous (
    regulation_id INTEGER PRIMARY KEY REFERENCES regulation_index(id) ON DELETE CASCADE,
    embedding     BLOB NOT NULL
);
INSERT INTO regulation_embeddings_v2_previous SELECT regulation_id, embedding FROM regulation_embeddings_v2;
DROP TABLE regulation_embeddings_v2;
ALTER TABLE regulation_embeddings_v2_previous RENAME TO regulation_embeddings_v2;
