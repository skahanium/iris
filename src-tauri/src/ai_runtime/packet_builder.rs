//! ContextPacket builder — assembles evidence packets from retrieval results.

use std::path::Path;

use rusqlite::Connection;

use crate::ai_runtime::retrieval_broker::{hybrid_retrieve, RetrievalLayers, RetrievalRequest};
use crate::ai_runtime::retrieval_scope::{
    resolve_retrieval_scope, ContextScopeDto, RetrievalScope,
};
use crate::ai_runtime::{AiScene, ContextPacket, ContextStatus, SourceType, TrustLevel};
use crate::error::AppResult;
use crate::knowledge::corpora::{
    corpus_for_path, load_corpora, packet_meta_for_entry, CorpusConfig,
};
use crate::llm::config::ContextStrategy;

/// Options for context packet assembly (budget-aware).
#[derive(Debug, Clone, Copy)]
pub struct ContextBuildOptions {
    pub max_results: usize,
    pub strategy: ContextStrategy,
    pub input_budget: usize,
}

impl ContextBuildOptions {
    pub fn from_scene_defaults(scene: AiScene) -> Self {
        Self {
            max_results: max_results_for_scene(scene),
            strategy: ContextStrategy::Hybrid,
            input_budget: 12_000,
        }
    }
}

/// Build context packets for a query in the given scene.
#[allow(clippy::too_many_arguments)]
pub fn build_context_packets(
    conn: &Connection,
    vault_path: &Path,
    scene: AiScene,
    note_path: Option<&str>,
    note_file_id: Option<i64>,
    query: &str,
    user_scope: &ContextScopeDto,
    opts: ContextBuildOptions,
) -> AppResult<(Vec<ContextPacket>, ContextStatus)> {
    let corpora = load_corpora(vault_path)?;
    let mut scope = resolve_retrieval_scope(&corpora, scene, user_scope);
    apply_exemplar_template_scope(scene, &corpora, &mut scope);

    let layers = layers_for_scene(scene);
    let max_results = opts.max_results;

    let request = RetrievalRequest {
        query: query.to_string(),
        max_results,
        layers,
        note_context: note_path.map(|s| s.to_string()),
        file_id_context: note_file_id,
        scope,
    };

    let mut packets = hybrid_retrieve(conn, &request)?;

    if matches!(opts.strategy, ContextStrategy::LongContext) {
        if let Some(path) = note_path {
            if let Some(full_packet) = note_fulltext_packet(vault_path, path, opts.input_budget) {
                packets.insert(0, full_packet);
            }
        }
    }
    annotate_packets_with_corpus(&corpora, &mut packets);

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

fn annotate_packets_with_corpus(corpora: &CorpusConfig, packets: &mut [ContextPacket]) {
    for packet in packets {
        let Some(path) = packet.source_path.as_deref() else {
            continue;
        };
        if let Some(entry) = corpus_for_path(corpora, path) {
            packet.corpus = Some(packet_meta_for_entry(entry));
        }
    }
}

/// When exemplar corpora exist, template search is limited to those prefixes (via post-filter).
fn apply_exemplar_template_scope(
    scene: AiScene,
    corpora: &CorpusConfig,
    scope: &mut RetrievalScope,
) {
    if !matches!(scene, AiScene::ExemplarLearning | AiScene::DraftingAssist) {
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

/// Scale retrieval top-k from effective input token budget.
pub fn max_results_from_budget(
    input_budget: usize,
    scene: AiScene,
    strategy: ContextStrategy,
) -> usize {
    let base = max_results_for_scene(scene);
    let scaled = (input_budget / 800).clamp(base, 80);
    if matches!(strategy, ContextStrategy::LongContext) {
        scaled.min(40)
    } else {
        scaled
    }
}

fn note_fulltext_packet(
    vault_path: &Path,
    note_path: &str,
    input_budget: usize,
) -> Option<ContextPacket> {
    let full_path = vault_path.join(note_path);
    let content = std::fs::read_to_string(&full_path).ok()?;
    let max_chars = (input_budget / 2).min(200_000);
    let excerpt = if content.chars().count() > max_chars {
        let truncated: String = content.chars().take(max_chars).collect();
        format!("{truncated}\n\n…（已截断至预算内）")
    } else {
        content
    };
    Some(ContextPacket {
        id: format!("note_full_{note_path}"),
        source_type: SourceType::Note,
        source_path: Some(note_path.to_string()),
        title: "当前笔记全文".into(),
        heading_path: None,
        source_span: None,
        content_hash: String::new(),
        excerpt,
        retrieval_reason: "long_context_note".into(),
        score: 1.0,
        trust_level: TrustLevel::UserNote,
        citation_label: "note_full".into(),
        stale: false,
        web: None,
        corpus: None,
    })
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
