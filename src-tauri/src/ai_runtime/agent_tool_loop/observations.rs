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
                let span = object
                    .get("sourceSpan")
                    .or_else(|| object.get("excerptWindow"));
                if let Some(span) = span {
                    let resource = object
                        .get("path")
                        .or_else(|| object.get("canonicalUrl"))
                        .or_else(|| object.get("sourcePath"))
                        .or_else(|| object.get("resourceId"));
                    let revision = object
                        .get("contentHash")
                        .or_else(|| object.get("content_hash"))
                        .or_else(|| object.get("snapshotHash"));
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
