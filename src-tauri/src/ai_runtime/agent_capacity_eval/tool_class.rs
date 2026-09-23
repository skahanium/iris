//! tool_class — part of the evaluator contract split out of `agent_capacity_eval.rs`.
//!
//! Moved verbatim; the parent re-exports it so existing paths keep working.

/// The evaluator admits the same dispatchable vault/context read tools as the
/// production catalog. Keeping this derivation here prevents a new safe local
/// retrieval tool from silently turning into a false hard-admission failure in
/// a live pilot.
#[cfg(test)]
use super::{ObservedEvalToolClass, PermissionDenialCategory, WebQueryBoundary};

pub(super) fn is_evaluation_local_read_tool(name: &str) -> bool {
    crate::ai_runtime::tool_catalog::catalog_find(name).is_some_and(|entry| {
        entry.implementation
            == crate::ai_runtime::tool_catalog::ToolImplementationStatus::Dispatchable
            && entry
                .required_capability_ids()
                .iter()
                .any(|capability| matches!(*capability, "vault.read" | "context.read"))
    })
}

/// Trusted runtime reads are model-visible under the immutable `runtime.read`
/// capability. They carry no user material or external evidence, so the
/// matrix records them as one closed operational class instead of falsely
/// treating a legitimate helper as an undeclared tool.
pub(super) fn is_evaluation_runtime_read_tool(name: &str) -> bool {
    crate::ai_runtime::tool_catalog::catalog_find(name).is_some_and(|entry| {
        entry.implementation
            == crate::ai_runtime::tool_catalog::ToolImplementationStatus::Dispatchable
            && entry.required_capability_ids().contains(&"runtime.read")
    })
}

/// Summarize all pre-dispatch witnesses for a local-plus-Web execution.
///
/// A clean retry cannot erase a previous blocked attempt: calibration needs to
/// know whether the model ever tried to disclose local material, while the
/// production boundary separately guarantees that the blocked query was never
/// sent. Missing witnesses are deliberately not treated as clean.
#[cfg(test)]
pub(crate) fn summarize_web_query_boundary(
    has_local_material: bool,
    web_search_observed: bool,
    witnesses: &[WebQueryBoundary],
) -> WebQueryBoundary {
    if !has_local_material || !web_search_observed {
        return WebQueryBoundary::NotApplicable;
    }
    if witnesses.contains(&WebQueryBoundary::BlockedLocalMaterial) {
        WebQueryBoundary::BlockedLocalMaterial
    } else if witnesses.contains(&WebQueryBoundary::ConfirmedClean) {
        WebQueryBoundary::ConfirmedClean
    } else {
        WebQueryBoundary::Unknown
    }
}

#[cfg(test)]
pub(crate) fn observed_eval_tool_class(tool_name: &str) -> ObservedEvalToolClass {
    if matches!(
        tool_name,
        "web.search" | "web.fetch" | "web_search" | "web_fetch"
    ) {
        ObservedEvalToolClass::WebSearch
    } else if is_evaluation_local_read_tool(tool_name) {
        ObservedEvalToolClass::LocalRead
    } else if is_evaluation_runtime_read_tool(tool_name) {
        ObservedEvalToolClass::RuntimeContext
    } else if tool_name.starts_with("external_") {
        ObservedEvalToolClass::ExternalRead
    } else if crate::ai_runtime::tool_catalog::catalog_find(tool_name).is_some() {
        ObservedEvalToolClass::OtherCatalogTool
    } else {
        ObservedEvalToolClass::UnknownTool
    }
}

pub(super) fn evaluation_local_read_tool_names() -> Vec<String> {
    crate::ai_runtime::tool_catalog::catalog_dispatchable_names()
        .into_iter()
        .filter(|name| is_evaluation_local_read_tool(name))
        .map(str::to_string)
        .collect()
}

/// Collapse an execution-time permission denial into a report-safe category.
/// This is deliberately derived from the catalog rather than a duplicated
/// name list, so adding a first-party tool cannot silently expose its label in
/// an evaluation artifact.
#[cfg(test)]
pub(crate) fn permission_denial_category(tool_name: &str) -> PermissionDenialCategory {
    let Some(entry) = crate::ai_runtime::tool_catalog::catalog_find(tool_name) else {
        return PermissionDenialCategory::UnknownTool;
    };
    let required = entry.required_capability_ids();
    if required
        .iter()
        .any(|capability| matches!(*capability, "vault.read" | "context.read"))
    {
        PermissionDenialCategory::LocalRead
    } else if required.contains(&"runtime.read") {
        PermissionDenialCategory::RuntimeContext
    } else if required.contains(&"web.search") {
        PermissionDenialCategory::WebSearch
    } else {
        PermissionDenialCategory::OtherCatalogTool
    }
}

/// Project only the lifecycle capabilities that have a one-to-one equivalent
/// in the closed model-tool contract. Other capability events are operational
/// telemetry, not evidence that the model called an undeclared evaluation tool;
/// the per-run tool audit below remains authoritative for those calls.
#[cfg(test)]
pub(crate) fn runtime_capability_to_eval_tool_name(value: &str) -> Option<&str> {
    match value {
        "web.search" | "web.fetch" => Some("web_search"),
        _ => None,
    }
}
