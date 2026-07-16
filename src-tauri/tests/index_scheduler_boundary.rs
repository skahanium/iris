#[test]
fn indexer_purity_contract_excludes_application_and_scheduler_dependencies() {
    let source = include_str!("../src/indexer/scan.rs");

    for forbidden in [
        "crate::app::AppState",
        "EmbeddingScheduler",
        "IndexEmbeddingMode",
        "enqueue_embedding",
        "notify_index_committed",
    ] {
        assert!(
            !source.contains(forbidden),
            "the SQLite indexer must stay pure; found forbidden dependency: {forbidden}"
        );
    }
}
