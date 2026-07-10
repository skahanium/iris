use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::ai_runtime::ContextPacket;

const RRF_K: f64 = 60.0;
const MMR_LAMBDA: f64 = 0.75;
const MAX_PACKETS_PER_SOURCE_IN_TOP_TEN: usize = 2;

#[derive(Debug)]
struct FusedCandidate {
    packet: ContextPacket,
    rrf_score: f64,
    representative_quality: f64,
}

/// Optional deterministic seam for reranking ordinary retrieval candidates.
///
/// The v1.2.6 default is intentionally a no-op. A future local reranker can
/// adjust candidate scores here without changing broker orchestration.
pub(super) trait CandidateReranker {
    fn rerank(&self, packets: &mut [ContextPacket]);
}

struct NoopCandidateReranker;

impl CandidateReranker for NoopCandidateReranker {
    fn rerank(&self, _packets: &mut [ContextPacket]) {}
}

/// Deterministic Rank v2 for hybrid retrieval.
///
/// Exact regulation matches are pinned before ordinary candidates. Ordinary
/// candidates are fused with weighted reciprocal-rank fusion instead of
/// multiplying incomparable raw scores, then selected with MMR and a per-file
/// cap to avoid filling the prompt with near-duplicate evidence.
pub(super) fn fuse_and_rank(packets: &mut Vec<ContextPacket>, max_results: usize) {
    fuse_and_rank_with_reranker(packets, max_results, &NoopCandidateReranker);
}

/// Fuse candidates after an optional ordinary-candidate reranking step.
pub(super) fn fuse_and_rank_with_reranker(
    packets: &mut Vec<ContextPacket>,
    max_results: usize,
    reranker: &dyn CandidateReranker,
) {
    if max_results == 0 {
        packets.clear();
        return;
    }

    let candidates = std::mem::take(packets);
    let (exact, mut ordinary): (Vec<_>, Vec<_>) = candidates
        .into_iter()
        .partition(|packet| is_exact_regulation(packet));

    reranker.rerank(&mut ordinary);
    let mut selected = select_exact_packets(exact, max_results);
    let remaining = max_results.saturating_sub(selected.len());
    if remaining > 0 {
        let fused = weighted_rrf(ordinary);
        selected.extend(select_with_mmr(fused, remaining));
    }

    *packets = selected;
}

fn is_exact_regulation(packet: &ContextPacket) -> bool {
    packet.retrieval_reason.starts_with("exact_")
}

fn layer_key(packet: &ContextPacket) -> &'static str {
    match packet.retrieval_reason.as_str() {
        reason if reason.starts_with("runtime_") => "runtime",
        reason if reason.starts_with("vector_regulation") => "vector_regulation",
        "fts_keyword_match" => "fts",
        reason if reason.starts_with("vector_chunk") => "vector_chunk",
        reason if reason.starts_with("vector_anchor") => "vector_anchor",
        reason if reason.starts_with("metadata_") => "metadata",
        reason if reason.starts_with("template_") => "template",
        reason if reason.starts_with("graph_") => "graph",
        _ => "other",
    }
}

fn layer_weight(layer: &str) -> f64 {
    match layer {
        "runtime" | "vector_regulation" => 1.20,
        "fts" | "vector_chunk" => 1.00,
        "vector_anchor" => 0.90,
        "metadata" => 0.80,
        "template" | "graph" => 0.70,
        _ => 0.60,
    }
}

fn corpus_multiplier(packet: &ContextPacket) -> f64 {
    let value: f64 = match packet.corpus.as_ref().map(|meta| meta.kind.as_str()) {
        Some("authority") => 1.15,
        Some("exemplar") => 1.00,
        Some("reference") => 0.95,
        Some("lookup") => 0.80,
        _ => 1.00,
    };
    value.clamp(0.75, 1.15)
}

fn raw_quality(packet: &ContextPacket) -> f64 {
    packet.score.clamp(0.0, 1.0) * corpus_multiplier(packet)
}

fn canonical_evidence_key(packet: &ContextPacket) -> String {
    let path = packet.source_path.as_deref().unwrap_or_default();
    if let Some(span) = &packet.source_span {
        return format!("{path}@{}:{}", span.start, span.end);
    }
    match packet.source_type {
        crate::ai_runtime::SourceType::Note => format!("note:{path}"),
        _ => format!(
            "{:?}:{path}:{}:{}",
            packet.source_type,
            packet.heading_path.as_deref().unwrap_or_default(),
            packet.citation_label
        ),
    }
}

fn source_key(packet: &ContextPacket) -> String {
    packet
        .source_path
        .clone()
        .unwrap_or_else(|| format!("packet:{}", packet.id))
}

fn select_exact_packets(packets: Vec<ContextPacket>, max_results: usize) -> Vec<ContextPacket> {
    let mut unique: BTreeMap<String, ContextPacket> = BTreeMap::new();
    for mut packet in packets {
        packet.score = 1.0;
        let key = canonical_evidence_key(&packet);
        let replace = unique.get(&key).map_or(true, |existing| {
            raw_quality(&packet) > raw_quality(existing)
        });
        if replace {
            unique.insert(key, packet);
        }
    }

    let mut ordered: Vec<_> = unique.into_values().collect();
    ordered.sort_by(|left, right| {
        raw_quality(right)
            .total_cmp(&raw_quality(left))
            .then_with(|| left.id.cmp(&right.id))
    });
    select_with_source_cap(ordered, max_results)
}

fn weighted_rrf(packets: Vec<ContextPacket>) -> Vec<FusedCandidate> {
    let mut per_layer: BTreeMap<&'static str, Vec<ContextPacket>> = BTreeMap::new();
    for packet in packets {
        per_layer
            .entry(layer_key(&packet))
            .or_default()
            .push(packet);
    }

    let mut fused: BTreeMap<String, FusedCandidate> = BTreeMap::new();
    for (layer, mut candidates) in per_layer {
        candidates.sort_by(|left, right| {
            raw_quality(right)
                .total_cmp(&raw_quality(left))
                .then_with(|| left.id.cmp(&right.id))
        });
        let weight = layer_weight(layer);
        for (position, packet) in candidates.into_iter().enumerate() {
            let rank = position + 1;
            let contribution = weight / (RRF_K + rank as f64);
            let key = canonical_evidence_key(&packet);
            let quality = raw_quality(&packet);
            match fused.get_mut(&key) {
                Some(current) => {
                    current.rrf_score += contribution;
                    if quality > current.representative_quality
                        || (quality == current.representative_quality
                            && packet.id < current.packet.id)
                    {
                        current.packet = packet;
                        current.representative_quality = quality;
                    }
                }
                None => {
                    fused.insert(
                        key,
                        FusedCandidate {
                            packet,
                            rrf_score: contribution,
                            representative_quality: quality,
                        },
                    );
                }
            }
        }
    }

    let mut result: Vec<_> = fused.into_values().collect();
    for candidate in &mut result {
        candidate.packet.score =
            (candidate.rrf_score * corpus_multiplier(&candidate.packet)).min(1.0);
    }
    result.sort_by(|left, right| {
        right
            .packet
            .score
            .total_cmp(&left.packet.score)
            .then_with(|| left.packet.id.cmp(&right.packet.id))
    });
    result
}

fn select_with_mmr(candidates: Vec<FusedCandidate>, max_results: usize) -> Vec<ContextPacket> {
    let mut remaining = candidates;
    let mut selected = Vec::with_capacity(max_results);
    let mut source_counts: HashMap<String, usize> = HashMap::new();
    let cap_window = max_results.min(10);

    while selected.len() < max_results {
        let apply_source_cap = selected.len() < cap_window;
        let next = best_mmr_candidate(&remaining, &selected, &source_counts, apply_source_cap)
            .or_else(|| best_mmr_candidate(&remaining, &selected, &source_counts, false));
        let Some(index) = next else {
            break;
        };
        let candidate = remaining.remove(index);
        let key = source_key(&candidate.packet);
        *source_counts.entry(key).or_default() += 1;
        selected.push(candidate.packet);
    }

    selected
}

fn best_mmr_candidate(
    candidates: &[FusedCandidate],
    selected: &[ContextPacket],
    source_counts: &HashMap<String, usize>,
    apply_source_cap: bool,
) -> Option<usize> {
    candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| {
            !apply_source_cap
                || source_counts
                    .get(&source_key(&candidate.packet))
                    .copied()
                    .unwrap_or_default()
                    < MAX_PACKETS_PER_SOURCE_IN_TOP_TEN
        })
        .max_by(|(_, left), (_, right)| {
            mmr_score(left, selected)
                .total_cmp(&mmr_score(right, selected))
                .then_with(|| right.packet.id.cmp(&left.packet.id))
        })
        .map(|(index, _)| index)
}

fn mmr_score(candidate: &FusedCandidate, selected: &[ContextPacket]) -> f64 {
    let redundancy = selected
        .iter()
        .map(|packet| excerpt_similarity(&candidate.packet.excerpt, &packet.excerpt))
        .fold(0.0_f64, f64::max);
    MMR_LAMBDA * candidate.packet.score - (1.0 - MMR_LAMBDA) * redundancy
}

fn select_with_source_cap(
    candidates: Vec<ContextPacket>,
    max_results: usize,
) -> Vec<ContextPacket> {
    let mut selected = Vec::with_capacity(max_results);
    let mut deferred = Vec::new();
    let mut source_counts: HashMap<String, usize> = HashMap::new();
    for candidate in candidates {
        if selected.len() == max_results {
            break;
        }
        let key = source_key(&candidate);
        if source_counts.get(&key).copied().unwrap_or_default() < MAX_PACKETS_PER_SOURCE_IN_TOP_TEN
        {
            *source_counts.entry(key).or_default() += 1;
            selected.push(candidate);
        } else {
            deferred.push(candidate);
        }
    }
    for candidate in deferred {
        if selected.len() == max_results {
            break;
        }
        selected.push(candidate);
    }
    selected
}

fn excerpt_similarity(left: &str, right: &str) -> f64 {
    let left_grams = character_ngrams(left);
    let right_grams = character_ngrams(right);
    if left_grams.is_empty() || right_grams.is_empty() {
        return 0.0;
    }
    let intersection = left_grams.intersection(&right_grams).count() as f64;
    let union = left_grams.union(&right_grams).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

fn character_ngrams(value: &str) -> BTreeSet<String> {
    let chars: Vec<_> = value.chars().filter(|ch| !ch.is_whitespace()).collect();
    match chars.len() {
        0 => BTreeSet::new(),
        1 => [chars[0].to_string()].into_iter().collect(),
        _ => chars
            .windows(2)
            .map(|window| window.iter().collect::<String>())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::{CorpusPacketMeta, SourceType, TrustLevel};

    fn packet(id: &str, kind: &str) -> ContextPacket {
        ContextPacket {
            id: id.into(),
            source_type: SourceType::Note,
            source_path: Some(format!("{id}.md")),
            title: id.into(),
            heading_path: None,
            source_span: None,
            content_hash: id.into(),
            excerpt: format!("evidence for {id}"),
            retrieval_reason: "fts_keyword_match".into(),
            score: 1.0,
            trust_level: TrustLevel::UserNote,
            citation_label: format!("[{id}]"),
            stale: false,
            web: None,
            corpus: Some(CorpusPacketMeta {
                id: kind.into(),
                name: kind.into(),
                kind: kind.into(),
                label: kind.into(),
                instruction: String::new(),
                can_be_authority: kind == "authority",
            }),
        }
    }

    #[test]
    fn role_weight_demotes_lookup_below_authority() {
        let mut packets = vec![packet("lookup", "lookup"), packet("authority", "authority")];

        fuse_and_rank(&mut packets, 10);

        assert_eq!(packets[0].id, "authority");
        assert!(packets[0].score > packets[1].score);
    }

    #[test]
    fn exact_regulations_are_pinned_before_non_exact_candidates() {
        let mut exact = packet("exact", "reference");
        exact.source_type = SourceType::Regulation;
        exact.retrieval_reason = "exact_regulation_lookup".into();
        exact.score = 0.01;

        let mut semantic = packet("semantic", "reference");
        semantic.retrieval_reason = "vector_regulation_match".into();
        semantic.score = 1.0;

        let mut packets = vec![semantic, exact];
        fuse_and_rank(&mut packets, 2);

        assert_eq!(packets[0].id, "exact");
    }

    #[test]
    fn repeated_layers_raise_one_evidence_packet_above_single_layer_candidates() {
        let mut fts = packet("fts", "exemplar");
        fts.source_path = Some("shared.md".into());
        fts.excerpt = "shared evidence".into();
        let mut vector = packet("vector", "exemplar");
        vector.source_path = Some("shared.md".into());
        vector.excerpt = "shared evidence".into();
        vector.retrieval_reason = "vector_chunk".into();
        let mut single = packet("single", "exemplar");
        single.retrieval_reason = "vector_chunk".into();

        let mut packets = vec![single, fts, vector];
        fuse_and_rank(&mut packets, 2);

        assert_eq!(packets[0].source_path.as_deref(), Some("shared.md"));
        assert_eq!(packets.len(), 2);
    }

    #[test]
    fn top_ten_caps_each_source_when_alternatives_exist() {
        let mut packets = Vec::new();
        for source in 0..5 {
            for ordinal in 0..3 {
                let mut candidate = packet(&format!("source-{source}-{ordinal}"), "reference");
                candidate.source_path = Some(format!("source-{source}.md"));
                candidate.score = 1.0 - (source * 3 + ordinal) as f64 * 0.01;
                packets.push(candidate);
            }
        }

        fuse_and_rank(&mut packets, 10);

        for source in 0..5 {
            assert!(
                packets
                    .iter()
                    .filter(|packet| packet.source_path.as_deref()
                        == Some(&format!("source-{source}.md")))
                    .count()
                    <= MAX_PACKETS_PER_SOURCE_IN_TOP_TEN
            );
        }
    }

    #[test]
    fn injected_reranker_can_change_ordinary_candidate_order() {
        struct PromoteSecond;

        impl CandidateReranker for PromoteSecond {
            fn rerank(&self, packets: &mut [ContextPacket]) {
                for packet in packets {
                    packet.score = if packet.id == "second" { 1.0 } else { 0.0 };
                }
            }
        }

        let mut first = packet("first", "reference");
        first.score = 0.8;
        let mut second = packet("second", "reference");
        second.score = 0.2;
        let mut packets = vec![first, second];
        fuse_and_rank_with_reranker(&mut packets, 2, &PromoteSecond);

        assert_eq!(packets[0].id, "second");
    }
    #[test]
    fn mmr_prefers_distinct_evidence_over_a_near_duplicate() {
        let mut first = packet("first", "exemplar");
        first.excerpt = "alpha beta gamma delta".into();
        let mut duplicate = packet("duplicate", "exemplar");
        duplicate.excerpt = "alpha beta gamma epsilon".into();
        duplicate.score = 0.99;
        let mut distinct = packet("distinct", "exemplar");
        distinct.excerpt = "omega lambda kappa".into();
        distinct.score = 0.90;

        let mut packets = vec![first, duplicate, distinct];
        fuse_and_rank(&mut packets, 2);

        assert!(packets.iter().any(|packet| packet.id == "first"));
        assert!(packets.iter().any(|packet| packet.id == "distinct"));
    }
}
