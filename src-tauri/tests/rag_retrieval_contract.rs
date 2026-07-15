use iris_lib::ai_runtime::retrieval_broker::{
    hybrid_retrieve_with_diagnostics, RetrievalLayerStatus, RetrievalLayers, RetrievalRequest,
};
use iris_lib::ai_runtime::retrieval_scope::RetrievalScope;
use iris_lib::ai_runtime::TrustLevel;
use iris_lib::embedding::engine::semantic_search;
use iris_lib::storage::migrate::migrate_up;
use rusqlite::Connection;

fn migrated_memory_connection() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory sqlite");
    migrate_up(&conn).expect("run migrations");
    conn
}

fn insert_chunk_for_path(conn: &Connection, path: &str, content: &str) {
    conn.execute(
        "INSERT INTO chunks
         (file_id, chunk_index, content, heading_path, source_start, source_end, content_hash)
         SELECT id, 0, ?2, 'Evidence', 0, ?3, ?4
         FROM files
         WHERE path = ?1",
        rusqlite::params![
            path,
            content,
            content.len() as i64,
            format!("chunk-hash-{path}")
        ],
    )
    .expect("insert citable chunk");
}

#[test]
fn semantic_search_empty_index_returns_empty_without_query_embedding() {
    let conn = migrated_memory_connection();

    let results = semantic_search(&conn, "empty query", 5)
        .expect("empty semantic index should be searchable");

    assert!(
        results.is_empty(),
        "empty chunk_embeddings table should return no semantic results"
    );
}

#[test]
fn fts_retrieval_builds_user_note_context_packet() {
    let conn = migrated_memory_connection();
    conn.execute(
        "INSERT INTO files (path, title, content_hash, word_count, created_at, updated_at)
         VALUES ('notes/rag.md', 'RAG Note', 'hash-rag', 2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    )
    .expect("insert file");
    conn.execute(
        "INSERT INTO files_fts (path, title, content)
         VALUES ('notes/rag.md', 'RAG Note', 'alpha evidence')",
        [],
    )
    .expect("insert fts row");
    conn.execute(
        "INSERT INTO chunks
         (file_id, chunk_index, content, heading_path, source_start, source_end, content_hash)
         VALUES (1, 0, 'alpha evidence', 'Evidence', 3, 17, 'chunk-hash')",
        [],
    )
    .expect("insert citable chunk");

    let request = RetrievalRequest {
        query: "alpha".into(),
        max_results: 5,
        layers: RetrievalLayers {
            fts: true,
            vector: false,
            graph: false,
            exact: false,
            template: false,
        },
        note_context: None,
        file_id_context: None,
        scope: RetrievalScope::default(),
        runtime_documents: Vec::new(),
        corpus_config: None,
    };

    let outcome =
        hybrid_retrieve_with_diagnostics(&conn, &request).expect("run diagnostic retrieval");

    assert_eq!(outcome.packets.len(), 1);
    assert_eq!(
        outcome.packets[0].source_path.as_deref(),
        Some("notes/rag.md")
    );
    assert_eq!(outcome.packets[0].trust_level, TrustLevel::UserNote);
    assert_eq!(outcome.packets[0].retrieval_reason, "fts_keyword_match");
    assert_eq!(outcome.packets[0].content_hash, "chunk-hash");
    assert!(outcome.packets[0].source_span.is_some());
    assert!(outcome
        .diagnostics
        .iter()
        .any(|diag| diag.layer == "fts" && diag.status == RetrievalLayerStatus::Ok));
}

#[test]
fn scope_is_applied_before_top_k_selection() {
    let conn = migrated_memory_connection();
    for (path, title) in [("outside.md", "Outside"), ("allowed.md", "Allowed")] {
        conn.execute(
            "INSERT INTO files (path, title, content_hash, word_count, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            rusqlite::params![path, title, format!("hash-{path}")],
        )
        .expect("insert file");
        conn.execute(
            "INSERT INTO files_fts (path, title, content) VALUES (?1, ?2, 'alpha evidence')",
            rusqlite::params![path, title],
        )
        .expect("insert fts row");
        insert_chunk_for_path(&conn, path, "alpha evidence");
    }

    let request = RetrievalRequest {
        query: "alpha".into(),
        max_results: 1,
        layers: RetrievalLayers {
            fts: true,
            vector: false,
            graph: false,
            exact: false,
            template: false,
        },
        note_context: None,
        file_id_context: None,
        scope: RetrievalScope {
            paths: vec!["allowed.md".into()],
            path_prefixes: Vec::new(),
            required_tags: Vec::new(),
        },
        runtime_documents: Vec::new(),
        corpus_config: None,
    };

    let outcome = hybrid_retrieve_with_diagnostics(&conn, &request).expect("run scoped retrieval");

    assert_eq!(outcome.packets.len(), 1);
    assert_eq!(
        outcome.packets[0].source_path.as_deref(),
        Some("allowed.md")
    );
}

#[test]
fn v2_embedding_generation_starts_legacy_ready_without_overwriting_legacy_embeddings() {
    let conn = migrated_memory_connection();

    let state: (String, String, i64, String) = conn
        .query_row(
            "SELECT active_model_id, target_model_id, target_dimension, phase
             FROM embedding_generation_state WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("v2 embedding generation state");
    let v2_table_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'chunk_embeddings_v2'",
            [],
            |row| row.get(0),
        )
        .expect("query v2 embedding table");

    assert_eq!(state.0, "fastembed/AllMiniLML6V2");
    assert_eq!(state.1, "Xenova/bge-small-zh-v1.5");
    assert_eq!(state.2, 512);
    assert_eq!(state.3, "legacy_ready");
    assert_eq!(v2_table_exists, 1);
}

#[test]
fn metadata_alias_retrieval_returns_note_without_polluting_body_fts() {
    let conn = migrated_memory_connection();
    conn.execute(
        "INSERT INTO files (path, title, content_hash, word_count, created_at, updated_at)
         VALUES ('notes/project.md', 'Project', 'hash-project', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    )
    .expect("insert file");
    conn.execute(
        "INSERT INTO files_metadata_fts (path, aliases, tags)
         VALUES ('notes/project.md', 'Phoenix initiative', 'work iris')",
        [],
    )
    .expect("insert metadata fts");
    insert_chunk_for_path(&conn, "notes/project.md", "Project summary evidence");

    let request = RetrievalRequest {
        query: "Phoenix".into(),
        max_results: 5,
        layers: RetrievalLayers {
            fts: true,
            vector: false,
            graph: false,
            exact: false,
            template: false,
        },
        note_context: None,
        file_id_context: None,
        scope: RetrievalScope::default(),
        runtime_documents: Vec::new(),
        corpus_config: None,
    };

    let outcome = hybrid_retrieve_with_diagnostics(&conn, &request).expect("metadata retrieval");

    assert_eq!(outcome.packets.len(), 1);
    assert_eq!(
        outcome.packets[0].source_path.as_deref(),
        Some("notes/project.md")
    );
    assert_eq!(
        outcome.packets[0].retrieval_reason,
        "metadata_alias_or_tag_match"
    );
}

#[test]
fn legacy_ready_generation_reports_vector_layer_as_not_ready_while_keyword_search_remains_available(
) {
    let conn = migrated_memory_connection();
    conn.execute(
        "INSERT INTO files (path, title, content_hash, word_count, created_at, updated_at)
         VALUES ('notes/legacy.md', 'Legacy', 'legacy-hash', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    )
    .expect("insert keyword-searchable note");
    conn.execute(
        "INSERT INTO files_fts (path, title, content)
         VALUES ('notes/legacy.md', 'Legacy', 'legacy alpha evidence')",
        [],
    )
    .expect("insert keyword index");
    insert_chunk_for_path(&conn, "notes/legacy.md", "legacy alpha evidence");
    let request = RetrievalRequest {
        query: "alpha".into(),
        max_results: 5,
        layers: RetrievalLayers {
            fts: true,
            vector: true,
            graph: false,
            exact: false,
            template: false,
        },
        note_context: None,
        file_id_context: None,
        scope: RetrievalScope::default(),
        runtime_documents: Vec::new(),
        corpus_config: None,
    };

    let outcome = hybrid_retrieve_with_diagnostics(&conn, &request).expect("run retrieval");
    let vector = outcome
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.layer == "vector")
        .expect("vector diagnostic");

    assert_eq!(vector.status, RetrievalLayerStatus::IndexNotReady);
    assert_eq!(
        vector.message.as_deref(),
        Some("BGE v2 embedding generation awaits idle upgrade")
    );
    assert!(outcome
        .packets
        .iter()
        .any(|packet| packet.retrieval_reason == "fts_keyword_match"));
}
