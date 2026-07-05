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

#[test]
fn semantic_search_empty_index_returns_empty_without_query_embedding() {
    let conn = migrated_memory_connection();

    let results =
        semantic_search(&conn, "任意查询", 5).expect("empty semantic index should be searchable");

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
    assert!(outcome
        .diagnostics
        .iter()
        .any(|diag| diag.layer == "fts" && diag.status == RetrievalLayerStatus::Ok));
}
