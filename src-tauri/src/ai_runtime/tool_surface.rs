//! Central tool-surface planner.
//!
//! Previously the decision whether `web_search` should be exposed to the model
//! was scattered across `run_intake` and `normal_run_service`. That caused
//! time-sensitive questions to run in Direct mode without a search tool, and
//! strict prefetch paths to hide the tool after a search already happened.
//! This module is the single place that turns request signals into a concrete
//! tool-surface plan.

use crate::ai_runtime::run_contract::{CapabilityId, Effort, FreshFactDomain};

/// How strongly the current user request depends on fresh external information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimeSensitivity {
    None,
    Current,
}

/// Web-tool instruction that should be injected into the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WebToolInstruction {
    None,
    MustSearchIfNeeded,
    AlreadyRetrievedDoNotDeny,
    NoWebDoNotFabricate,
}

/// Inputs needed to decide the tool surface for one Run.
#[derive(Debug, Clone)]
pub(crate) struct ToolSurfaceInput {
    pub(crate) web_enabled: bool,
    pub(crate) time_sensitive: TimeSensitivity,
    pub(crate) effort: Effort,
    pub(crate) web_prefetched: bool,
    pub(crate) authorized_capabilities: Vec<CapabilityId>,
}

/// The resolved tool-surface plan for one Run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolSurfacePlan {
    pub(crate) effort: Effort,
    pub(crate) expose_web_search: bool,
    pub(crate) web_prefetched: bool,
    pub(crate) web_instruction: WebToolInstruction,
    /// Exact model-visible tool names frozen for this Run. Filled by the
    /// production orchestrator after combining the plan with the authorized
    /// ToolRegistry and Run context constraints.
    pub(crate) tool_names: Vec<String>,
}

pub(crate) struct ToolSurfacePlanner;

impl ToolSurfacePlanner {
    pub(crate) fn plan(input: ToolSurfaceInput) -> ToolSurfacePlan {
        let has_web_capability = input
            .authorized_capabilities
            .iter()
            .any(|capability| capability.as_str() == "web.search");
        let web_allowed = input.web_enabled && has_web_capability;
        let time_sensitive = input.time_sensitive == TimeSensitivity::Current;

        if !web_allowed {
            return ToolSurfacePlan {
                effort: input.effort,
                expose_web_search: false,
                web_prefetched: input.web_prefetched,
                web_instruction: if time_sensitive {
                    WebToolInstruction::NoWebDoNotFabricate
                } else {
                    WebToolInstruction::None
                },
                tool_names: Vec::new(),
            };
        }

        // Evidence was already prefetched by the strict path. The final model
        // does not need to search again; it must only be told not to deny that
        // retrieval happened. Keeping the tool hidden also preserves the
        // ability to finish with a non-tool-capable model.
        if input.web_prefetched {
            return ToolSurfacePlan {
                effort: input.effort,
                expose_web_search: false,
                web_prefetched: true,
                web_instruction: WebToolInstruction::AlreadyRetrievedDoNotDeny,
                tool_names: Vec::new(),
            };
        }

        // A time-sensitive request with Web enabled must be able to search.
        // Direct mode never exposes web_search, so promote it to ToolLoop.
        let effort = if time_sensitive && input.effort == Effort::Direct {
            Effort::ToolLoop
        } else {
            input.effort
        };

        // Direct mode does not expose web_search; ToolLoop/Durable do.
        let expose_web_search = effort != Effort::Direct;
        let web_instruction = if time_sensitive {
            WebToolInstruction::MustSearchIfNeeded
        } else {
            WebToolInstruction::None
        };

        ToolSurfacePlan {
            effort,
            expose_web_search,
            web_prefetched: false,
            web_instruction,
            tool_names: Vec::new(),
        }
    }
}

/// Project whether a frozen fresh-fact domain is time-sensitive.
///
/// This deliberately replaces message-keyword scanning: the same deterministic
/// classifier used at intake is the single source of truth for time windows.
pub(crate) fn classify_time_sensitivity(domain: FreshFactDomain) -> TimeSensitivity {
    match domain {
        FreshFactDomain::None | FreshFactDomain::Runtime => TimeSensitivity::None,
        _ => TimeSensitivity::Current,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::run_contract::CapabilityId;

    fn web_capabilities() -> Vec<CapabilityId> {
        vec![
            CapabilityId::new("model.text"),
            CapabilityId::new("web.search"),
        ]
    }

    fn no_web_capabilities() -> Vec<CapabilityId> {
        vec![CapabilityId::new("model.text")]
    }

    #[test]
    fn web_enabled_time_sensitive_promotes_direct_to_tool_loop_and_exposes_search() {
        let plan = ToolSurfacePlanner::plan(ToolSurfaceInput {
            web_enabled: true,
            time_sensitive: TimeSensitivity::Current,
            effort: Effort::Direct,
            web_prefetched: false,
            authorized_capabilities: web_capabilities(),
        });

        assert_eq!(plan.effort, Effort::ToolLoop);
        assert!(plan.expose_web_search);
        assert_eq!(plan.web_instruction, WebToolInstruction::MustSearchIfNeeded);
    }

    #[test]
    fn web_disabled_time_sensitive_does_not_expose_search_and_forbids_fabrication() {
        let plan = ToolSurfacePlanner::plan(ToolSurfaceInput {
            web_enabled: false,
            time_sensitive: TimeSensitivity::Current,
            effort: Effort::Direct,
            web_prefetched: false,
            authorized_capabilities: no_web_capabilities(),
        });

        assert_eq!(plan.effort, Effort::Direct);
        assert!(!plan.expose_web_search);
        assert_eq!(
            plan.web_instruction,
            WebToolInstruction::NoWebDoNotFabricate
        );
    }

    #[test]
    fn prefetched_evidence_keeps_tool_hidden_but_denies_nothing() {
        let plan = ToolSurfacePlanner::plan(ToolSurfaceInput {
            web_enabled: true,
            time_sensitive: TimeSensitivity::Current,
            effort: Effort::Direct,
            web_prefetched: true,
            authorized_capabilities: web_capabilities(),
        });

        assert_eq!(plan.effort, Effort::Direct);
        assert!(!plan.expose_web_search);
        assert_eq!(
            plan.web_instruction,
            WebToolInstruction::AlreadyRetrievedDoNotDeny
        );
    }

    #[test]
    fn non_time_sensitive_web_enabled_stays_in_original_effort() {
        let plan = ToolSurfacePlanner::plan(ToolSurfaceInput {
            web_enabled: true,
            time_sensitive: TimeSensitivity::None,
            effort: Effort::Direct,
            web_prefetched: false,
            authorized_capabilities: web_capabilities(),
        });

        assert_eq!(plan.effort, Effort::Direct);
        assert!(!plan.expose_web_search);
        assert_eq!(plan.web_instruction, WebToolInstruction::None);
    }

    #[test]
    fn classify_time_sensitivity_projects_from_fresh_fact_domain() {
        assert_eq!(
            classify_time_sensitivity(FreshFactDomain::Entertainment),
            TimeSensitivity::Current
        );
        assert_eq!(
            classify_time_sensitivity(FreshFactDomain::Weather),
            TimeSensitivity::Current
        );
        assert_eq!(
            classify_time_sensitivity(FreshFactDomain::None),
            TimeSensitivity::None
        );
    }
}
