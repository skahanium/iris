CREATE TABLE IF NOT EXISTS semantic_anchor_embeddings_v2 (
    anchor_id   INTEGER PRIMARY KEY REFERENCES semantic_anchors(id) ON DELETE CASCADE,
    embedding   BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS regulation_embeddings_v2 (
    regulation_id INTEGER PRIMARY KEY REFERENCES regulation_index(id) ON DELETE CASCADE,
    embedding     BLOB NOT NULL
);