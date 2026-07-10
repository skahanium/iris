//! Hybrid retrieval broker: request contract and layer wiring.

use rusqlite::Connection;

use crate::ai_runtime::retrieval_scope::RetrievalScope;
use crate::ai_runtime::{ContextPacket, RuntimeDocumentSnapshot};
use crate::error::AppResult;
use crate::knowledge::corpora::CorpusConfig;

#[path = "retrieval_broker/query_hash.rs"]
mod query_hash_impl;
pub use query_hash_impl::query_hash;

#[path = "retrieval_broker/diagnostics.rs"]
mod diagnostics_impl;
pub use diagnostics_impl::{
    hybrid_retrieve_with_diagnostics, RetrievalLayerDiagnostic, RetrievalLayerStatus,
    RetrievalOutcome,
};

#[path = "retrieval_broker/exact.rs"]
mod exact_impl;
#[path = "retrieval_broker/fts.rs"]
mod fts_impl;
#[path = "retrieval_broker/graph.rs"]
mod graph_impl;
#[path = "retrieval_broker/metadata.rs"]
mod metadata_impl;
#[path = "retrieval_broker/rank.rs"]
mod rank_impl;
#[path = "retrieval_broker/template.rs"]
mod template_impl;
#[path = "retrieval_broker/vector.rs"]
mod vector_impl;

use exact_impl::search_exact_regulation;
use fts_impl::search_fts;
use graph_impl::search_graph_neighbors;
use metadata_impl::search_metadata;
use rank_impl::fuse_and_rank;
use template_impl::search_template;
use vector_impl::{search_vector_anchors, search_vector_chunks, search_vector_regulations};

/// Complete request contract for one hybrid retrieval call.
#[derive(Debug, Clone)]
pub struct RetrievalRequest {
    pub query: String,
    pub max_results: usize,
    pub layers: RetrievalLayers,
    pub note_context: Option<String>,
    pub file_id_context: Option<i64>,
    pub scope: RetrievalScope,
    pub runtime_documents: Vec<RuntimeDocumentSnapshot>,
    pub corpus_config: Option<CorpusConfig>,
}

/// Independently enabled retrieval layers.
#[derive(Debug, Clone)]
pub struct RetrievalLayers {
    pub fts: bool,
    pub vector: bool,
    pub graph: bool,
    pub exact: bool,
    pub template: bool,
}

impl Default for RetrievalLayers {
    fn default() -> Self {
        Self {
            fts: true,
            vector: true,
            graph: true,
            exact: true,
            template: false,
        }
    }
}

/// Run hybrid retrieval and return fused evidence packets.
pub fn hybrid_retrieve(
    conn: &Connection,
    request: &RetrievalRequest,
) -> AppResult<Vec<ContextPacket>> {
    Ok(hybrid_retrieve_with_diagnostics(conn, request)?.packets)
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(max_chars).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrieval_request_default_layers_are_enabled_as_expected() {
        let request = RetrievalRequest {
            query: "test".into(),
            max_results: 10,
            layers: RetrievalLayers::default(),
            note_context: None,
            file_id_context: None,
            scope: RetrievalScope::default(),
            runtime_documents: Vec::new(),
            corpus_config: None,
        };
        assert!(request.layers.fts);
        assert!(request.layers.vector);
        assert!(request.layers.graph);
        assert!(request.layers.exact);
        assert!(!request.layers.template);
    }

    #[test]
    fn empty_database_returns_no_packets() {
        let conn = rusqlite::Connection::open_in_memory().expect("open database");
        let request = RetrievalRequest {
            query: "article 6".into(),
            max_results: 10,
            layers: RetrievalLayers::default(),
            note_context: None,
            file_id_context: None,
            scope: RetrievalScope::default(),
            runtime_documents: Vec::new(),
            corpus_config: None,
        };
        assert!(hybrid_retrieve(&conn, &request)
            .expect("retrieve")
            .is_empty());
    }

    #[test]
    fn truncate_preserves_short_values_and_marks_long_values() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate(&"a".repeat(100), 20).chars().count(), 23);
    }
}
