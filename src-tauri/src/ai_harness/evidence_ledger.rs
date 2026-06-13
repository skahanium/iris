//! Unified evidence ingestion: dedupe, stable citation labels, ordering, compaction.

use crate::ai_runtime::harness_support::compact_evidence;
use crate::ai_runtime::ContextPacket;
use crate::error::{AppError, AppResult};

const MAX_PACKETS: usize = 100;

/// In-memory ledger for a single harness / workflow run.
#[derive(Debug, Clone, Default)]
pub struct EvidenceLedger {
    packets: Vec<ContextPacket>,
    label_seq: u32,
}

impl EvidenceLedger {
    pub fn new(initial: Vec<ContextPacket>) -> Self {
        let mut ledger = Self {
            packets: Vec::new(),
            label_seq: 0,
        };
        ledger.ingest_many(initial);
        ledger
    }

    pub fn ingest_many(&mut self, incoming: Vec<ContextPacket>) {
        for p in incoming {
            self.ingest(p);
        }
    }

    pub fn ingest(&mut self, mut packet: ContextPacket) {
        if self.packets.iter().any(|p| p.id == packet.id) {
            return;
        }
        if packet.citation_label.is_empty() {
            self.label_seq += 1;
            packet.citation_label = format!("[{}]", self.label_seq);
        }
        self.packets.push(packet);
    }

    pub fn packets(&self) -> &[ContextPacket] {
        &self.packets
    }

    pub fn into_packets(mut self, token_budget: usize) -> Vec<ContextPacket> {
        self.sort_and_cap();
        compact_evidence(&mut self.packets, token_budget);
        self.packets
    }

    /// Re-resolve packet IDs; errors when user-selected IDs are all invalid.
    pub fn resolve_selected_packet_ids(
        &self,
        selected_ids: &[String],
        available: &[ContextPacket],
    ) -> AppResult<(Vec<String>, Option<String>)> {
        if selected_ids.is_empty() {
            return Ok((vec![], None));
        }
        let available_ids: std::collections::HashSet<_> =
            available.iter().map(|p| p.id.as_str()).collect();
        let resolved: Vec<String> = selected_ids
            .iter()
            .filter(|id| available_ids.contains(id.as_str()))
            .cloned()
            .collect();
        if resolved.is_empty() {
            return Err(AppError::msg(
                "所选证据包均不可用，请重新选择证据或刷新检索后再执行",
            ));
        }
        if resolved.len() == selected_ids.len() {
            return Ok((resolved, None));
        }
        Ok((
            resolved,
            Some("正式执行时部分预览证据不可用，已使用当前检索结果。".into()),
        ))
    }

    fn sort_and_cap(&mut self) {
        self.packets.sort_by(|a, b| {
            let rank_a = source_rank(&a.source_type);
            let rank_b = source_rank(&b.source_type);
            rank_a.cmp(&rank_b).then(
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        });
        if self.packets.len() > MAX_PACKETS {
            self.packets.truncate(MAX_PACKETS);
        }
    }
}

fn source_rank(source_type: &crate::ai_runtime::SourceType) -> u8 {
    use crate::ai_runtime::SourceType;
    match source_type {
        SourceType::Note | SourceType::Anchor => 0,
        SourceType::Regulation => 1,
        SourceType::Template | SourceType::Session => 2,
        SourceType::Web => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::harness_support::estimate_tokens;
    use crate::ai_runtime::{SourceType, TrustLevel};

    fn pkt(id: &str, score: f64, source: SourceType) -> ContextPacket {
        ContextPacket {
            id: id.into(),
            source_type: source,
            source_path: None,
            title: id.into(),
            heading_path: None,
            source_span: None,
            content_hash: String::new(),
            excerpt: "x".repeat(40),
            retrieval_reason: String::new(),
            score,
            trust_level: TrustLevel::UserNote,
            citation_label: String::new(),
            stale: false,
            web: None,
        }
    }

    #[test]
    fn dedupe_by_id() {
        let mut ledger = EvidenceLedger::new(vec![pkt("a", 1.0, SourceType::Note)]);
        ledger.ingest(pkt("a", 2.0, SourceType::Note));
        assert_eq!(ledger.packets().len(), 1);
    }

    #[test]
    fn stable_citation_labels() {
        let mut ledger = EvidenceLedger::default();
        ledger.ingest(pkt("a", 1.0, SourceType::Note));
        ledger.ingest(pkt("b", 1.0, SourceType::Note));
        assert_eq!(ledger.packets()[0].citation_label, "[1]");
        assert_eq!(ledger.packets()[1].citation_label, "[2]");
    }

    #[test]
    fn validate_selected_reports_refresh() {
        let available = vec![pkt("a", 1.0, SourceType::Note)];
        let ledger = EvidenceLedger::new(available);
        let (ids, notice) = ledger
            .resolve_selected_packet_ids(&["a".into(), "missing".into()], ledger.packets())
            .unwrap();
        assert!(notice.is_some());
        assert_eq!(ids, vec!["a".to_string()]);
    }

    #[test]
    fn compact_respects_budget() {
        let mut ledger = EvidenceLedger::new(vec![pkt("a", 1.0, SourceType::Note)]);
        ledger.packets[0].excerpt = "word ".repeat(200);
        let out = ledger.into_packets(estimate_tokens("short"));
        assert!(out[0].excerpt.contains("压缩") || out[0].excerpt.len() < 400);
    }
}
