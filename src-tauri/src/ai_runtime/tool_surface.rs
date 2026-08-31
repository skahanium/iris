//! Central tool-surface planner.
//!
//! Previously the decision whether `web_search` should be exposed to the model
//! was scattered across `run_intake` and `normal_run_service`. That caused
//! time-sensitive questions to run in Direct mode without a search tool, and
//! strict prefetch paths to hide the tool after a search already happened.
//! This module is the single place that turns request signals into a concrete
//! tool-surface plan.

use crate::ai_runtime::run_contract::{CapabilityId, Effort};

/// Web-tool instruction that should be injected into the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WebToolInstruction {
    None,
    MustSearchIfNeeded,
    NoWebDoNotFabricate,
}

/// Inputs needed to decide the tool surface for one Run.
#[derive(Debug, Clone)]
pub(crate) struct ToolSurfaceInput {
    pub(crate) web_enabled: bool,
    /// Whether the frozen Run contract requires current-Run Web evidence.
    /// This is intentionally a contract fact, not a domain classifier result.
    pub(crate) requires_current_web_evidence: bool,
    pub(crate) effort: Effort,
    pub(crate) authorized_capabilities: Vec<CapabilityId>,
}

/// The resolved tool-surface plan for one Run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolSurfacePlan {
    pub(crate) effort: Effort,
    pub(crate) expose_web_search: bool,
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
        let requires_current_web_evidence = input.requires_current_web_evidence;

        if !web_allowed {
            return ToolSurfacePlan {
                effort: input.effort,
                expose_web_search: false,
                web_instruction: if requires_current_web_evidence {
                    WebToolInstruction::NoWebDoNotFabricate
                } else {
                    WebToolInstruction::None
                },
                tool_names: Vec::new(),
            };
        }

        // A Web capability is a read surface the user has authorized for this
        // Run.  Direct mode would hide it from the model, turning
        // `WebPreferred` into a Host-side promise that cannot be exercised.
        // Use the same bounded loop for both preferred and required Web; the
        // verification requirement only changes the completion contract.
        let effort = if input.effort == Effort::Direct {
            Effort::ToolLoop
        } else {
            input.effort
        };

        let expose_web_search = true;
        let web_instruction = if requires_current_web_evidence {
            WebToolInstruction::MustSearchIfNeeded
        } else {
            WebToolInstruction::None
        };

        ToolSurfacePlan {
            effort,
            expose_web_search,
            web_instruction,
            tool_names: Vec::new(),
        }
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
    fn current_web_evidence_contract_promotes_direct_to_tool_loop_and_exposes_search() {
        let plan = ToolSurfacePlanner::plan(ToolSurfaceInput {
            web_enabled: true,
            requires_current_web_evidence: true,
            effort: Effort::Direct,
            authorized_capabilities: web_capabilities(),
        });

        assert_eq!(plan.effort, Effort::ToolLoop);
        assert!(plan.expose_web_search);
        assert_eq!(plan.web_instruction, WebToolInstruction::MustSearchIfNeeded);
    }

    #[test]
    fn current_web_evidence_contract_without_authorization_forbids_fabrication() {
        let plan = ToolSurfacePlanner::plan(ToolSurfaceInput {
            web_enabled: false,
            requires_current_web_evidence: true,
            effort: Effort::Direct,
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
    fn webpreferred_contract_promotes_direct_to_tool_loop_and_exposes_search() {
        let plan = ToolSurfacePlanner::plan(ToolSurfaceInput {
            web_enabled: true,
            requires_current_web_evidence: false,
            effort: Effort::Direct,
            authorized_capabilities: web_capabilities(),
        });

        assert_eq!(plan.effort, Effort::ToolLoop);
        assert!(plan.expose_web_search);
        assert_eq!(plan.web_instruction, WebToolInstruction::None);
    }

    #[test]
    fn evidence_obligation_not_a_domain_controls_the_web_instruction() {
        let plan = ToolSurfacePlanner::plan(ToolSurfaceInput {
            web_enabled: true,
            requires_current_web_evidence: true,
            effort: Effort::ToolLoop,
            authorized_capabilities: web_capabilities(),
        });

        assert_eq!(plan.web_instruction, WebToolInstruction::MustSearchIfNeeded);
    }
}
