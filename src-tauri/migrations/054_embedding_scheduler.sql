CREATE TABLE embedding_generation_state_next (
    singleton           INTEGER PRIMARY KEY CHECK (singleton = 1),
    active_model_id     TEXT NOT NULL,
    target_model_id     TEXT NOT NULL,
    target_dimension    INTEGER NOT NULL CHECK (target_dimension > 0),
    phase               TEXT NOT NULL CHECK (phase IN ('legacy_ready', 'running', 'paused', 'ready', 'failed')),
    indexed_items       INTEGER NOT NULL DEFAULT 0 CHECK (indexed_items >= 0),
    total_items         INTEGER NOT NULL DEFAULT 0 CHECK (total_items >= 0),
    last_error          TEXT,
    failure_code        TEXT,
    automatic_attempted INTEGER NOT NULL DEFAULT 0 CHECK (automatic_attempted IN (0, 1)),
    updated_at          TEXT NOT NULL
);

INSERT INTO embedding_generation_state_next (
    singleton, active_model_id, target_model_id, target_dimension, phase,
    indexed_items, total_items, last_error, failure_code, automatic_attempted, updated_at
)
SELECT singleton, active_model_id, target_model_id, target_dimension,
       CASE
           WHEN phase = 'rebuilding' AND indexed_items = 0 AND total_items = 0 THEN 'legacy_ready'
           WHEN phase = 'rebuilding' THEN 'failed'
           ELSE phase
       END,
       indexed_items, total_items,
       CASE
           WHEN phase = 'rebuilding' AND NOT (indexed_items = 0 AND total_items = 0)
               THEN 'Embedding rebuild interrupted'
           ELSE last_error
       END,
       CASE
           WHEN phase = 'rebuilding' AND NOT (indexed_items = 0 AND total_items = 0)
               THEN 'interrupted_migration'
           WHEN phase = 'failed' THEN 'embedding_failed'
           ELSE NULL
       END,
       0, updated_at
FROM embedding_generation_state;

DROP TABLE embedding_generation_state;
ALTER TABLE embedding_generation_state_next RENAME TO embedding_generation_state;

CREATE TABLE chunk_embeddings_v2_next (
    chunk_id           INTEGER PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
    embedding          BLOB NOT NULL,
    source_fingerprint TEXT NOT NULL DEFAULT '',
    model_id           TEXT NOT NULL DEFAULT '',
    dimension          INTEGER NOT NULL DEFAULT 0
);
INSERT INTO chunk_embeddings_v2_next (chunk_id, embedding)
SELECT chunk_id, embedding FROM chunk_embeddings_v2;
DROP TABLE chunk_embeddings_v2;
ALTER TABLE chunk_embeddings_v2_next RENAME TO chunk_embeddings_v2;

CREATE TABLE semantic_anchor_embeddings_v2_next (
    anchor_id          INTEGER PRIMARY KEY REFERENCES semantic_anchors(id) ON DELETE CASCADE,
    embedding          BLOB NOT NULL,
    source_fingerprint TEXT NOT NULL DEFAULT '',
    model_id           TEXT NOT NULL DEFAULT '',
    dimension          INTEGER NOT NULL DEFAULT 0
);
INSERT INTO semantic_anchor_embeddings_v2_next (anchor_id, embedding)
SELECT anchor_id, embedding FROM semantic_anchor_embeddings_v2;
DROP TABLE semantic_anchor_embeddings_v2;
ALTER TABLE semantic_anchor_embeddings_v2_next RENAME TO semantic_anchor_embeddings_v2;

CREATE TABLE regulation_embeddings_v2_next (
    regulation_id      INTEGER PRIMARY KEY REFERENCES regulation_index(id) ON DELETE CASCADE,
    embedding          BLOB NOT NULL,
    source_fingerprint TEXT NOT NULL DEFAULT '',
    model_id           TEXT NOT NULL DEFAULT '',
    dimension          INTEGER NOT NULL DEFAULT 0
);
INSERT INTO regulation_embeddings_v2_next (regulation_id, embedding)
SELECT regulation_id, embedding FROM regulation_embeddings_v2;
DROP TABLE regulation_embeddings_v2;
ALTER TABLE regulation_embeddings_v2_next RENAME TO regulation_embeddings_v2;
