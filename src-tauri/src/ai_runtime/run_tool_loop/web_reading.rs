//! Run-owned snapshots and exact, citation-bearing model reading windows.
use super::*;
use crate::ai_runtime::web_evidence_broker::WebEvidenceItem;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub(super) struct PageSnapshot {
    item: WebEvidenceItem,
    hash: String,
    length: usize,
}

impl PageSnapshot {
    pub(super) fn new(mut item: WebEvidenceItem) -> Option<Self> {
        let body = item.fetched_excerpt.as_ref()?;
        if body.trim().is_empty() || item.failure_reason.is_some() {
            return None;
        }
        let bounded: String = body.chars().take(12_000).collect();
        if bounded.chars().count() < body.chars().count() {
            item.completeness = crate::llm::fetch_web_page::PageContentCompleteness::Bounded;
        }
        let hash = crate::cas::hash::content_hash_str(&bounded);
        let length = bounded.chars().count();
        item.fetched_excerpt = Some(bounded);
        item.snippet.clear();
        item.title = truncate_web_field(&item.title, 256);
        Some(Self { item, hash, length })
    }
}

struct ReadingPage {
    item: WebEvidenceItem,
    hash: String,
    length: usize,
    start: usize,
}

impl ReadingPage {
    fn window(&self) -> Value {
        let returned = self
            .item
            .fetched_excerpt
            .as_deref()
            .unwrap_or_default()
            .chars()
            .count();
        let end = self.start + returned;
        json!({"startChar":self.start,"endChar":end,"returnedChars":returned,
            "unit":"unicode_scalar","limitChars":MAX_WEB_EXCERPT_CHARS,
            "nextStartChar":(end < self.length).then_some(end),
            "truncated":end < self.length,"snapshotChars":self.length,
            "upstreamCompleteness":self.item.completeness})
    }
}

impl NormalRunToolExecutor<'_> {
    pub(super) fn cached_fetch_urls(&self, urls: &[String]) -> AppResult<Vec<String>> {
        let state = self
            .run_web_evidence
            .lock()
            .map_err(|_| AppError::run(SafeRunErrorCode::EvidenceLockFailed))?;
        Ok(urls
            .iter()
            .filter(|url| !state.snapshots.contains_key(*url))
            .take(state.max_evidence.saturating_sub(state.snapshots.len()))
            .cloned()
            .collect())
    }

    pub(super) fn observe_web_pages(
        &self,
        urls: &[String],
        start: usize,
        items: Vec<WebEvidenceItem>,
        state_version: u64,
        elapsed: Duration,
    ) -> AppResult<ToolCallResult> {
        let (mut pages, mut statuses) = {
            let mut state = self
                .run_web_evidence
                .lock()
                .map_err(|_| AppError::run(SafeRunErrorCode::EvidenceLockFailed))?;
            for item in items {
                let key = normalize_fetch_url(&item.canonical_url);
                if !urls.contains(&key)
                    || state.snapshots.contains_key(&key)
                    || state.snapshots.len() >= state.max_evidence
                {
                    continue;
                }
                if let Some(snapshot) = PageSnapshot::new(item) {
                    state.snapshots.insert(key, snapshot);
                }
            }
            let mut pages = Vec::new();
            let mut statuses = Vec::new();
            for url in urls {
                let Some(snapshot) = state.snapshots.get(url) else {
                    statuses.push(json!({"canonicalUrl":url,"status":"snapshot_unavailable",
                        "recovery":"read_startChar_zero_to_acquire_snapshot"}));
                    continue;
                };
                if start >= snapshot.length {
                    statuses.push(json!({"canonicalUrl":url,"snapshotHash":snapshot.hash,
                        "status":if start == snapshot.length {"eof"} else {"range_out_of_bounds"},
                        "excerptWindow":{"startChar":start,"endChar":start,"nextStartChar":null,
                            "snapshotChars":snapshot.length,"upstreamCompleteness":snapshot.item.completeness}}));
                    continue;
                }
                let mut item = snapshot.item.clone();
                item.fetched_excerpt = Some(
                    item.fetched_excerpt
                        .as_deref()
                        .unwrap_or_default()
                        .chars()
                        .skip(start)
                        .take(MAX_WEB_EXCERPT_CHARS)
                        .collect(),
                );
                pages.push(ReadingPage {
                    item,
                    hash: snapshot.hash.clone(),
                    length: snapshot.length,
                    start,
                });
            }
            (pages, statuses)
        };
        let reservation = if pages.is_empty() {
            None
        } else {
            WebEvidenceReservation::reserve(Arc::clone(&self.run_web_evidence))?
        };
        let capacity = reservation
            .as_ref()
            .map_or(0, WebEvidenceReservation::capacity);
        for page in pages.drain(capacity.min(pages.len())..) {
            statuses.push(json!({"canonicalUrl":page.item.canonical_url,"status":"evidence_budget_exhausted"}));
        }
        // Build the identical shape used after registration, reserving maximal
        // SQLite identifiers and Run ordinals. Only excerpt prefixes may shrink.
        let placeholders = vec![(i64::MAX, i64::MAX); pages.len()];
        let missing_requirement = if self.requires_corroborated_web_evidence() {
            json!("second_independent_domain")
        } else {
            json!("one_fetched_body")
        };
        loop {
            let output = reading_output(
                &pages,
                &statuses,
                &placeholders,
                elapsed,
                &missing_requirement,
            )?;
            // Reserve the widest legal loop projection so the model-facing
            // message cannot re-truncate an excerpt this sizing pass accepted.
            let envelope = json!({
                "success": false,
                "output": output,
                "error": "web_read_unavailable",
                "loopObservation": crate::ai_runtime::agent_tool_loop::LoopProjection::widest().to_json(),
            });
            if envelope.to_string().chars().count() <= MAX_WEB_TOOL_RESULT_CHARS {
                break;
            }
            let Some(page) = pages.iter_mut().max_by_key(|page| {
                page.item
                    .fetched_excerpt
                    .as_deref()
                    .unwrap_or_default()
                    .chars()
                    .count()
            }) else {
                return Err(AppError::run(SafeRunErrorCode::ToolLoopLimit));
            };
            let body = page.item.fetched_excerpt.as_deref().unwrap_or_default();
            let length = body.chars().count();
            if length <= 1 {
                return Err(AppError::run(SafeRunErrorCode::ToolLoopLimit));
            }
            page.item.fetched_excerpt = Some(
                body.chars()
                    .take(length.saturating_sub(128).max(1))
                    .collect(),
            );
        }
        let selected = pages
            .iter()
            .map(|page| page.item.clone())
            .collect::<Vec<_>>();
        let ids = register_model_web_evidence(
            &self.state.db,
            self.accepted,
            self.context,
            (self.subagent_depth == 0).then_some(self.sink),
            state_version,
            &selected,
            capacity,
        )?;
        if ids.len() != pages.len() {
            return Err(AppError::run(SafeRunErrorCode::WebEvidenceInvalid));
        }
        let mut bindings = Vec::new();
        for id in &ids {
            let links = AgentEvidenceRepository::list_selected_current_run_web_citation_links(
                &self.state.db,
                &self.accepted.run_id,
                &[*id],
            )?;
            let link = links
                .first()
                .ok_or_else(|| AppError::run(SafeRunErrorCode::UnverifiedWebCitation))?;
            bindings.push((*id, link.index));
        }
        if let Some(reservation) = reservation {
            reservation.commit(&ids)?;
        }
        self.local_evidence_ids
            .lock()
            .map_err(|_| AppError::run(SafeRunErrorCode::EvidenceLockFailed))?
            .extend(ids);
        self.record_web_evidence_quality(&selected)?;
        let success = !pages.is_empty() || statuses.iter().all(|item| item["status"] == "eof");
        if !pages.is_empty() {
            self.set_web_failure(None)?;
        }
        Ok(ToolCallResult {
            tool_name: WEB_FETCH_TOOL_NAME.into(),
            success,
            output: reading_output(
                &pages,
                &statuses,
                &bindings,
                elapsed,
                &if self.has_web_evidence() {
                    Value::Null
                } else {
                    missing_requirement
                },
            )?,
            duration_ms: bounded_duration_ms(elapsed),
            tokens_used: None,
            error: (!success).then(|| "web_read_unavailable".into()),
        })
    }
}

fn reading_output(
    pages: &[ReadingPage],
    statuses: &[Value],
    bindings: &[(i64, i64)],
    elapsed: Duration,
    remaining_requirement: &Value,
) -> AppResult<Value> {
    let items = pages
        .iter()
        .map(|page| page.item.clone())
        .collect::<Vec<_>>();
    let packets =
        crate::ai_runtime::web_evidence_broker::web_evidence_items_to_packets_with_excerpt_limit(
            "",
            &items,
            MAX_WEB_EXCERPT_CHARS,
        );
    let mut results = Vec::new();
    for ((mut packet, page), (id, index)) in packets.into_iter().zip(pages).zip(bindings) {
        // Set the typed packet before serialization: ContextPacket's existing
        // wire contract uses citation_label, not a parallel camelCase field.
        packet.id = format!("run-web-{index}");
        packet.citation_label = format!("[W{index}]");
        packet.content_hash = page.hash.clone();
        let mut value = serde_json::to_value(packet)?;
        value["evidenceId"] = json!(id);
        value["canonicalUrl"] = json!(page.item.canonical_url);
        value["excerptWindow"] = page.window();
        results.push(value);
    }
    results.extend_from_slice(statuses);
    let single_window = (results.len() == 1)
        .then(|| results[0].get("excerptWindow").cloned())
        .flatten();
    let mut output = json!({"results":results,"evidenceIds":bindings.iter().map(|(id,_)|id).collect::<Vec<_>>(),
        "resourceIds":bindings.iter().map(|(_,index)|format!("run-web-{index}")).collect::<Vec<_>>(),
        "count":pages.len(),"sourceDomains":distinct_source_domains(items.iter()),"scopeCheck":WEB_SCOPE_CHECK_NOTE,
        "failedUrls":statuses.iter().filter(|item|item["status"]!="eof").map(|item|item["canonicalUrl"].clone()).collect::<Vec<_>>(),
        "observationDepth":if pages.is_empty() {"snapshot_status"} else {"fetched_body"},"requiresFetchForCitation":false,
        "remainingEvidenceRequirement":remaining_requirement,
        "resultBudget":{"format":"context_packets_only","rawEvidenceOmitted":true},
        "budgetRemaining":remaining_web_tool_budget_ms(elapsed) > 0});
    if let Some(window) = single_window {
        output["excerptWindow"] = window;
    }
    Ok(output)
}
