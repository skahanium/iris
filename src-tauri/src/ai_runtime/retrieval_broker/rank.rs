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

    fn corpus_role_weight(p: &ContextPacket) -> f64 {
        match p.corpus.as_ref().map(|meta| meta.kind.as_str()) {
            Some("authority") => 1.08,
            Some("exemplar") => 1.0,
            Some("reference") => 0.92,
            Some("lookup") => 0.72,
            _ => 1.0,
        }
    }

    // Apply weighted scores
    for p in packets.iter_mut() {
        let weight = layer_weight(p) * corpus_role_weight(p);
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
    diversify_by_source_path(packets, max_results);
}

fn diversify_by_source_path(packets: &mut Vec<ContextPacket>, max_results: usize) {
    if packets.len() <= max_results {
        return;
    }

    let mut ranked = std::mem::take(packets);
    let mut selected = Vec::with_capacity(max_results);
    let mut seen_paths: HashSet<String> = HashSet::new();

    while selected.len() < max_results && !ranked.is_empty() {
        let next_idx = ranked
            .iter()
            .position(|packet| match packet.source_path.as_ref() {
                Some(path) => !seen_paths.contains(path),
                None => true,
            })
            .unwrap_or(0);
        let packet = ranked.remove(next_idx);
        if let Some(path) = &packet.source_path {
            seen_paths.insert(path.clone());
        }
        selected.push(packet);
    }

    *packets = selected;
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
            excerpt: "excerpt".into(),
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
    fn diversify_keeps_multiple_files_in_top_results() {
        let mut packets = vec![
            packet("same-1", "exemplar"),
            packet("same-2", "exemplar"),
            packet("same-3", "exemplar"),
            packet("other", "exemplar"),
        ];
        packets[0].source_path = Some("same.md".into());
        packets[1].source_path = Some("same.md".into());
        packets[2].source_path = Some("same.md".into());
        packets[3].source_path = Some("other.md".into());
        packets[0].score = 1.0;
        packets[1].score = 0.99;
        packets[2].score = 0.98;
        packets[3].score = 0.70;

        fuse_and_rank(&mut packets, 3);

        assert_eq!(packets.len(), 3);
        assert!(
            packets
                .iter()
                .any(|packet| packet.source_path.as_deref() == Some("other.md")),
            "top results should avoid all coming from one file"
        );
    }
}
