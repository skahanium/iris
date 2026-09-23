//! Bounded execution records replace the former success-only fingerprint set.
use super::*;

#[derive(Clone)]
pub(super) struct ExecutionRecord {
    pub result: ToolCallResult,
    pub call_id: String,
    pub compacted: bool,
}

impl ExecutionRecord {
    pub fn blocks_execution(&self) -> bool {
        self.result.success && !self.compacted
    }
}

pub(crate) fn safe_progress_identities(output: &serde_json::Value) -> Vec<String> {
    fn collect(value: &serde_json::Value, identities: &mut HashSet<String>) {
        match value {
            serde_json::Value::Array(items) => {
                for item in items {
                    collect(item, identities);
                }
            }
            serde_json::Value::Object(object) => {
                // Failed members of a partially successful web batch have a
                // URL for recovery, but no newly observed resource content.
                if object.get("canonicalUrl").is_some()
                    && object
                        .get("status")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|status| {
                            matches!(
                                status,
                                "snapshot_unavailable"
                                    | "evidence_budget_exhausted"
                                    | "range_out_of_bounds"
                            )
                        })
                {
                    return;
                }
                // ContextPacket uses snake_case; read_note uses camelCase.
                // Web packets carry a null local source_span plus their own
                // excerptWindow, so only an actual object selects the range.
                let span = ["sourceSpan", "source_span", "excerptWindow"]
                    .iter()
                    .filter_map(|key| object.get(*key))
                    .find(|value| value.is_object());
                if let Some(span) = span {
                    let resource = [
                        "path",
                        "canonicalUrl",
                        "sourcePath",
                        "source_path",
                        "resourceId",
                    ]
                    .iter()
                    .filter_map(|key| object.get(*key).and_then(serde_json::Value::as_str))
                    .find(|value| !value.is_empty());
                    let revision = ["contentHash", "content_hash", "snapshotHash"]
                        .iter()
                        .filter_map(|key| object.get(*key).and_then(serde_json::Value::as_str))
                        .find(|value| !value.is_empty());
                    let start = span
                        .get("start")
                        .or_else(|| span.get("startChar"))
                        .and_then(serde_json::Value::as_u64);
                    let end = span
                        .get("end")
                        .or_else(|| span.get("endChar"))
                        .and_then(serde_json::Value::as_u64);
                    if let (Some(resource), Some(revision), Some(start), Some(end)) =
                        (resource, revision, start, end)
                    {
                        if end > start {
                            identities.insert(
                                serde_json::json!([
                                    resource,
                                    revision,
                                    start,
                                    end,
                                    if span.get("startChar").is_some() {
                                        "unicode_scalar"
                                    } else {
                                        "utf8_byte"
                                    }
                                ])
                                .to_string(),
                            );
                        }
                        return;
                    }
                }
                for (key, child) in object {
                    if matches!(
                        key.as_str(),
                        "resourceId"
                            | "resourceIds"
                            | "resource_id"
                            | "resource_ids"
                            | "canonicalUrl"
                            | "canonicalUrls"
                            | "canonical_url"
                            | "canonical_urls"
                            | "contentHash"
                            | "content_hash"
                            | "revision"
                            | "fileHash"
                            | "file_hash"
                            | "targetFileHash"
                            | "target_file_hash"
                    ) {
                        if let Some(text) = child.as_str().filter(|text| !text.is_empty()) {
                            identities.insert(text.to_string());
                        } else if let Some(values) = child.as_array() {
                            identities.extend(
                                values
                                    .iter()
                                    .filter_map(serde_json::Value::as_str)
                                    .map(str::to_string),
                            );
                        }
                    }
                    collect(child, identities);
                }
            }
            _ => {}
        }
    }
    let mut identities = HashSet::new();
    collect(output, &mut identities);
    let mut identities: Vec<_> = identities.into_iter().collect();
    identities.sort();
    identities
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::{ContextPacket, SourceSpan, SourceType, TrustLevel};
    use serde_json::json;

    #[test]
    fn plan_a_review_context_packets_bind_progress_to_path_hash_and_byte_range() {
        let packet = ContextPacket {
            id: "chunk-1".into(),
            source_type: SourceType::Note,
            source_path: Some("notes/a.md".into()),
            title: "A".into(),
            heading_path: None,
            source_span: Some(SourceSpan { start: 0, end: 3 }),
            content_hash: "same-revision".into(),
            excerpt: "中".into(),
            retrieval_reason: "keyword".into(),
            score: 1.0,
            trust_level: TrustLevel::UserNote,
            citation_label: "[L1]".into(),
            stale: false,
            web: None,
            corpus: None,
        };
        let identity =
            |packet: &ContextPacket| safe_progress_identities(&json!({"results":[packet]}));
        let first = identity(&packet);
        assert_eq!(first.len(), 1);
        let mut next = packet.clone();
        next.source_span = Some(SourceSpan { start: 3, end: 6 });
        assert_ne!(first, identity(&next), "another chunk is new information");
        next = packet.clone();
        next.source_path = Some("notes/b.md".into());
        assert_ne!(first, identity(&next), "identical bytes do not merge files");
        next = packet.clone();
        next.content_hash = "updated-revision".into();
        assert_ne!(first, identity(&next));
        next = packet.clone();
        next.id = "different-search-row".into();
        next.score = 0.5;
        assert_eq!(first, identity(&next), "reranking is not new reading");
        next.source_span = Some(SourceSpan { start: 3, end: 3 });
        assert!(identity(&next).is_empty(), "an empty range is not progress");
    }

    #[test]
    fn plan_a_review_web_packet_uses_window_despite_null_local_span() {
        let packet = |start| {
            json!({"source_span":null,"source_path":null,
            "canonicalUrl":"https://example.com/page","content_hash":"snapshot",
            "excerptWindow":{"startChar":start,"endChar":start+3}})
        };
        assert_ne!(
            safe_progress_identities(&packet(0)),
            safe_progress_identities(&packet(3))
        );
        assert_eq!(safe_progress_identities(&packet(0)).len(), 1);
    }

    #[test]
    fn plan_a_review_unavailable_web_results_do_not_add_progress_to_a_partial_batch() {
        let page = json!({"canonicalUrl":"https://example.com/page","content_hash":"snapshot",
            "excerptWindow":{"startChar":0,"endChar":3}});
        let expected = safe_progress_identities(&json!({"results":[page]}));
        for status in [
            "snapshot_unavailable",
            "evidence_budget_exhausted",
            "range_out_of_bounds",
        ] {
            let partial = json!({"results":[page, {"canonicalUrl":"https://example.com/missing","status":status}]});
            assert_eq!(
                safe_progress_identities(&partial),
                expected,
                "{status} is not a read"
            );
        }
    }
}
