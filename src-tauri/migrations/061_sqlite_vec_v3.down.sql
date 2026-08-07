DROP TRIGGER IF EXISTS regulation_embeddings_v2_vec_v3_delete;
DROP TRIGGER IF EXISTS regulation_embeddings_v2_vec_v3_update;
DROP TRIGGER IF EXISTS regulation_embeddings_v2_vec_v3_insert;
DROP TRIGGER IF EXISTS semantic_anchor_embeddings_v2_vec_v3_delete;
DROP TRIGGER IF EXISTS semantic_anchor_embeddings_v2_vec_v3_update;
DROP TRIGGER IF EXISTS semantic_anchor_embeddings_v2_vec_v3_insert;
DROP TRIGGER IF EXISTS chunk_embeddings_v2_vec_v3_delete;
DROP TRIGGER IF EXISTS chunk_embeddings_v2_vec_v3_update;
DROP TRIGGER IF EXISTS chunk_embeddings_v2_vec_v3_insert;

DROP TABLE IF EXISTS vec_regulations_v3;
DROP TABLE IF EXISTS vec_anchors_v3;
DROP TABLE IF EXISTS vec_chunks_v3;
