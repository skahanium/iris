use std::hash::{Hash, Hasher};

use super::RetrievalRequest;

/// Build the stable cache key for a retrieval request.
///
/// The key intentionally ignores note-specific context so the same query and
/// layer settings can share cached packets across notes.
pub fn query_hash(request: &RetrievalRequest) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    request.query.hash(&mut hasher);
    request.max_results.hash(&mut hasher);
    request.layers.fts.hash(&mut hasher);
    request.layers.vector.hash(&mut hasher);
    request.layers.graph.hash(&mut hasher);
    request.layers.exact.hash(&mut hasher);
    request.layers.template.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use crate::ai_runtime::retrieval_scope::RetrievalScope;

    use super::super::RetrievalLayers;
    use super::*;

    #[test]
    fn ignores_note_context_but_tracks_layer_switches() {
        let base = RetrievalRequest {
            query: "contract risk".into(),
            max_results: 8,
            layers: RetrievalLayers::default(),
            note_context: Some("A.md".into()),
            file_id_context: Some(1),
            scope: RetrievalScope::default(),
            runtime_documents: Vec::new(),
            corpus_config: None,
        };
        let mut same_query_different_context = base.clone();
        same_query_different_context.note_context = Some("B.md".into());
        same_query_different_context.file_id_context = Some(2);
        assert_eq!(query_hash(&base), query_hash(&same_query_different_context));

        let mut different_layers = base.clone();
        different_layers.layers.vector = !different_layers.layers.vector;
        assert_ne!(query_hash(&base), query_hash(&different_layers));
    }
}
