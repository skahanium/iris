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
    packets.truncate(max_results);
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
}
