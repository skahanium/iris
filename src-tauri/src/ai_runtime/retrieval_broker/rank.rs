use std::collections::HashSet;

use crate::ai_runtime::ContextPacket;

/// Weighted score fusion: normalize scores by layer, apply weights, deduplicate.
pub(super) fn fuse_and_rank(packets: &mut Vec<ContextPacket>, max_results: usize) {
    // Layer weights: exact > regulation > user_note > anchor > chunk > template > graph
    fn layer_weight(p: &ContextPacket) -> f64 {
        match p.retrieval_reason.as_str() {
            r if r.starts_with("exact_") => 1.0,
            r if r.starts_with("vector_regulation") => 0.95,
            "fts_keyword_match" => 0.85,
            r if r.starts_with("vector_chunk") => 0.80,
            r if r.starts_with("vector_anchor") => 0.75,
            r if r.starts_with("template_") => 0.70,
            r if r.starts_with("graph_") => 0.60,
            _ => 0.50,
        }
    }

    // Apply weighted scores
    for p in packets.iter_mut() {
        let weight = layer_weight(p);
        p.score = (p.score * weight).min(1.0);
    }

    // Sort by weighted score descending
    packets.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Deduplicate: keep highest-scoring occurrence of each id (HashSet, not dedup_by)
    let mut seen = HashSet::new();
    packets.retain(|p| seen.insert(p.id.clone()));
    packets.truncate(max_results);
}
