//! ContextPacket builder — assembles evidence packets from retrieval results.

use std::path::Path;

use rusqlite::Connection;

use crate::ai_runtime::retrieval_broker::{hybrid_retrieve, RetrievalLayers, RetrievalRequest};
use crate::ai_runtime::retrieval_scope::{resolve_retrieval_scope, ContextScopeDto, RetrievalScope};
use crate::ai_runtime::{AiScene, ContextPacket, ContextStatus};
use crate::error::AppResult;
use crate::knowledge::corpora::{load_corpora, CorpusConfig};

/// Build context packets for a query in the given scene.
pub fn build_context_packets(
    conn: &Connection,
    vault_path: &Path,
    scene: AiScene,
    note_path: Option<&str>,
    note_file_id: Option<i64>,
    query: &str,
    user_scope: &ContextScopeDto,
) -> AppResult<(Vec<ContextPacket>, ContextStatus)> {
    let corpora = load_corpora(vault_path)?;
    let mut scope = resolve_retrieval_scope(&corpora, scene, user_scope);
    apply_exemplar_template_scope(scene, &corpora, &mut scope);

    let layers = layers_for_scene(scene);
    let max_results = max_results_for_scene(scene);

    let request = RetrievalRequest {
        query: query.to_string(),
        max_results,
        layers,
        note_context: note_path.map(|s| s.to_string()),
        file_id_context: note_file_id,
        scope,
    };

    let packets = hybrid_retrieve(conn, &request)?;

    let status = ContextStatus {
        regulations_loaded: packets
            .iter()
            .filter(|p| matches!(p.source_type, crate::ai_runtime::SourceType::Regulation))
            .count(),
        model_essays_loaded: packets
            .iter()
            .filter(|p| p.retrieval_reason.starts_with("template_"))
            .count(),
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

/// When exemplar corpora exist, template search is limited to those prefixes (via post-filter).
fn apply_exemplar_template_scope(scene: AiScene, corpora: &CorpusConfig, scope: &mut RetrievalScope) {
    if !matches!(
        scene,
        AiScene::ExemplarLearning | AiScene::DraftingAssist
    ) {
        return;
    }
    let exemplar_prefixes: Vec<String> = corpora
        .corpus
        .iter()
        .filter(|c| c.kind == "exemplar")
        .map(|c| crate::knowledge::corpora::normalize_prefix(&c.path_prefix))
        .filter(|p| !p.is_empty())
        .collect();
    if exemplar_prefixes.is_empty() {
        return;
    }
    if scope.is_unrestricted() {
        for p in exemplar_prefixes {
            scope.path_prefixes.push(p);
        }
    }
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
