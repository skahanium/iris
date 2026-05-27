//! ContextPacket builder — assembles evidence packets from retrieval results.

use rusqlite::Connection;

use crate::ai_runtime::retrieval_broker::{hybrid_retrieve, RetrievalLayers, RetrievalRequest};
use crate::ai_runtime::{AiScene, ContextPacket, ContextStatus};
use crate::error::AppResult;

/// Build context packets for a query in the given scene.
pub fn build_context_packets(
    conn: &Connection,
    scene: AiScene,
    note_path: Option<&str>,
    note_file_id: Option<i64>,
    query: &str,
) -> AppResult<(Vec<ContextPacket>, ContextStatus)> {
    let layers = layers_for_scene(scene);
    let max_results = max_results_for_scene(scene);

    let request = RetrievalRequest {
        query: query.to_string(),
        max_results,
        layers,
        note_context: note_path.map(|s| s.to_string()),
        file_id_context: note_file_id,
    };

    let packets = hybrid_retrieve(conn, &request)?;

    let status = ContextStatus {
        regulations_loaded: packets
            .iter()
            .filter(|p| matches!(p.source_type, crate::ai_runtime::SourceType::Regulation))
            .count(),
        model_essays_loaded: 0, // Phase C+
        anchors_loaded: packets
            .iter()
            .filter(|p| matches!(p.source_type, crate::ai_runtime::SourceType::Anchor))
            .count(),
        links_loaded: packets
            .iter()
            .filter(|p| p.retrieval_reason.starts_with("graph_"))
            .count(),
        total_tokens_estimate: packets
            .iter()
            .map(|p| p.excerpt.chars().count())
            .sum::<usize>()
            / 2,
    };

    Ok((packets, status))
}

fn layers_for_scene(scene: AiScene) -> RetrievalLayers {
    match scene {
        AiScene::KnowledgeLookup => RetrievalLayers {
            fts: true,
            vector: true,
            graph: true,
            exact: true,
            template: false,
        },
        AiScene::ExemplarLearning => RetrievalLayers {
            fts: true,
            vector: true,
            graph: true,
            exact: false,
            template: true,
        },
        AiScene::DraftingAssist => RetrievalLayers {
            fts: true,
            vector: true,
            graph: true,
            exact: true,
            template: true,
        },
        AiScene::ResearchSynthesis => RetrievalLayers {
            fts: true,
            vector: true,
            graph: true,
            exact: true,
            template: true,
        },
    }
}

fn max_results_for_scene(scene: AiScene) -> usize {
    match scene {
        AiScene::KnowledgeLookup => 15,
        AiScene::ExemplarLearning => 10,
        AiScene::DraftingAssist => 15,
        AiScene::ResearchSynthesis => 30,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_per_scene_are_non_empty() {
        for scene in [
            AiScene::KnowledgeLookup,
            AiScene::ExemplarLearning,
            AiScene::DraftingAssist,
            AiScene::ResearchSynthesis,
        ] {
            let layers = layers_for_scene(scene);
            assert!(layers.fts || layers.vector || layers.graph || layers.exact);
        }
    }

    #[test]
    fn research_scene_gets_most_results() {
        assert!(
            max_results_for_scene(AiScene::ResearchSynthesis)
                > max_results_for_scene(AiScene::KnowledgeLookup)
        );
    }
}
