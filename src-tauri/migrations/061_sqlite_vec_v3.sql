-- sqlite-vec v3 is a derived index. The three v2 embedding caches remain the
-- canonical rebuild sources and are mirrored exclusively by these triggers.

CREATE VIRTUAL TABLE vec_chunks_v3 USING vec0(
    chunk_id INTEGER PRIMARY KEY,
    embedding float[512] distance_metric=cosine,
    file_id INTEGER
);

CREATE VIRTUAL TABLE vec_anchors_v3 USING vec0(
    anchor_id INTEGER PRIMARY KEY,
    embedding float[512] distance_metric=cosine,
    file_id INTEGER
);

CREATE VIRTUAL TABLE vec_regulations_v3 USING vec0(
    regulation_id INTEGER PRIMARY KEY,
    embedding float[512] distance_metric=cosine,
    file_id INTEGER
);

INSERT INTO vec_chunks_v3 (chunk_id, embedding, file_id)
SELECT cache.chunk_id, cache.embedding, chunks.file_id
FROM chunk_embeddings_v2 AS cache
INNER JOIN chunks ON chunks.id = cache.chunk_id
WHERE cache.dimension = 512
  AND length(cache.embedding) = 2048;

INSERT INTO vec_anchors_v3 (anchor_id, embedding, file_id)
SELECT cache.anchor_id, cache.embedding, anchors.file_id
FROM semantic_anchor_embeddings_v2 AS cache
INNER JOIN semantic_anchors AS anchors ON anchors.id = cache.anchor_id
WHERE cache.dimension = 512
  AND length(cache.embedding) = 2048;

INSERT INTO vec_regulations_v3 (regulation_id, embedding, file_id)
SELECT cache.regulation_id, cache.embedding, regulations.file_id
FROM regulation_embeddings_v2 AS cache
INNER JOIN regulation_index AS regulations ON regulations.id = cache.regulation_id
WHERE cache.dimension = 512
  AND length(cache.embedding) = 2048;

CREATE TRIGGER chunk_embeddings_v2_vec_v3_insert
AFTER INSERT ON chunk_embeddings_v2
WHEN NEW.dimension = 512 AND length(NEW.embedding) = 2048
BEGIN
    INSERT INTO vec_chunks_v3 (chunk_id, embedding, file_id)
    SELECT NEW.chunk_id, NEW.embedding, chunks.file_id
    FROM chunks
    WHERE chunks.id = NEW.chunk_id;
END;

CREATE TRIGGER chunk_embeddings_v2_vec_v3_update
AFTER UPDATE ON chunk_embeddings_v2
BEGIN
    DELETE FROM vec_chunks_v3 WHERE chunk_id = OLD.chunk_id;
    INSERT INTO vec_chunks_v3 (chunk_id, embedding, file_id)
    SELECT NEW.chunk_id, NEW.embedding, chunks.file_id
    FROM chunks
    WHERE chunks.id = NEW.chunk_id
      AND NEW.dimension = 512
      AND length(NEW.embedding) = 2048;
END;

CREATE TRIGGER chunk_embeddings_v2_vec_v3_delete
AFTER DELETE ON chunk_embeddings_v2
BEGIN
    DELETE FROM vec_chunks_v3 WHERE chunk_id = OLD.chunk_id;
END;

CREATE TRIGGER semantic_anchor_embeddings_v2_vec_v3_insert
AFTER INSERT ON semantic_anchor_embeddings_v2
WHEN NEW.dimension = 512 AND length(NEW.embedding) = 2048
BEGIN
    INSERT INTO vec_anchors_v3 (anchor_id, embedding, file_id)
    SELECT NEW.anchor_id, NEW.embedding, anchors.file_id
    FROM semantic_anchors AS anchors
    WHERE anchors.id = NEW.anchor_id;
END;

CREATE TRIGGER semantic_anchor_embeddings_v2_vec_v3_update
AFTER UPDATE ON semantic_anchor_embeddings_v2
BEGIN
    DELETE FROM vec_anchors_v3 WHERE anchor_id = OLD.anchor_id;
    INSERT INTO vec_anchors_v3 (anchor_id, embedding, file_id)
    SELECT NEW.anchor_id, NEW.embedding, anchors.file_id
    FROM semantic_anchors AS anchors
    WHERE anchors.id = NEW.anchor_id
      AND NEW.dimension = 512
      AND length(NEW.embedding) = 2048;
END;

CREATE TRIGGER semantic_anchor_embeddings_v2_vec_v3_delete
AFTER DELETE ON semantic_anchor_embeddings_v2
BEGIN
    DELETE FROM vec_anchors_v3 WHERE anchor_id = OLD.anchor_id;
END;

CREATE TRIGGER regulation_embeddings_v2_vec_v3_insert
AFTER INSERT ON regulation_embeddings_v2
WHEN NEW.dimension = 512 AND length(NEW.embedding) = 2048
BEGIN
    INSERT INTO vec_regulations_v3 (regulation_id, embedding, file_id)
    SELECT NEW.regulation_id, NEW.embedding, regulations.file_id
    FROM regulation_index AS regulations
    WHERE regulations.id = NEW.regulation_id;
END;

CREATE TRIGGER regulation_embeddings_v2_vec_v3_update
AFTER UPDATE ON regulation_embeddings_v2
BEGIN
    DELETE FROM vec_regulations_v3 WHERE regulation_id = OLD.regulation_id;
    INSERT INTO vec_regulations_v3 (regulation_id, embedding, file_id)
    SELECT NEW.regulation_id, NEW.embedding, regulations.file_id
    FROM regulation_index AS regulations
    WHERE regulations.id = NEW.regulation_id
      AND NEW.dimension = 512
      AND length(NEW.embedding) = 2048;
END;

CREATE TRIGGER regulation_embeddings_v2_vec_v3_delete
AFTER DELETE ON regulation_embeddings_v2
BEGIN
    DELETE FROM vec_regulations_v3 WHERE regulation_id = OLD.regulation_id;
END;
