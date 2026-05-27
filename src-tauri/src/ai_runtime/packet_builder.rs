//! ContextPacket builder — assembles evidence packets from retrieval results.
//!
//! Phase A: skeleton — returns empty packet set with status.
//! Phase B+: wires in RetrievalBroker, semantic anchors, regulation index, etc.

use crate::ai_runtime::{AiScene, ContextPacket, ContextStatus};

/// Phase A placeholder: returns an empty assembled context.
pub fn build_context_packets(
    _scene: AiScene,
    _note_path: Option<&str>,
    _query: &str,
) -> (Vec<ContextPacket>, ContextStatus) {
    let status = ContextStatus {
        regulations_loaded: 0,
        model_essays_loaded: 0,
        anchors_loaded: 0,
        links_loaded: 0,
        total_tokens_estimate: 0,
    };
    (vec![], status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_a_returns_empty_packets() {
        let (packets, status) = build_context_packets(
            AiScene::KnowledgeLookup,
            None,
            "test query",
        );
        assert!(packets.is_empty());
        assert_eq!(status.total_tokens_estimate, 0);
    }
}
