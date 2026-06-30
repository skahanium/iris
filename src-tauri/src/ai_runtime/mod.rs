//! Iris Agent Task Runtime 閳?task policy, context planning, tool permission, trace.
//!
//! Shared data types live in [`crate::ai_types`] and are re-exported here
//! for backward compatibility.

pub use crate::ai_types::*;

// 閳光偓閳光偓閳光偓 Modules that remain in ai_runtime (coordination layer) 閳光偓
pub mod agent_permissions;
pub mod agent_task;
pub mod agent_task_policy;
pub mod capability_resolver;
pub mod circuit_breaker;
pub mod classified_retrieval;
pub mod classified_session;
pub mod context_cache;
pub mod context_planner;
pub mod conversation_memory;
pub mod deliberation;
pub mod environment;
pub mod execution_plan;
pub mod guardrails;
pub mod mcp_host_runtime;
pub mod mcp_runtime_registry;
pub mod model_gateway;
pub mod model_registry;
pub mod packet_builder;
pub mod packet_cache;
pub mod permission_decision;
pub mod persona_resolver;
pub mod prompt_builder;
pub mod prompt_profile;
pub mod research_state;
pub mod retrieval_broker;
pub mod retrieval_scope;
pub mod runtime_context;
pub mod sandbox_profile;
pub mod session;
pub mod session_evidence;
pub mod skill_install_service;
pub mod skill_registry;
pub mod skill_trust_policy;
pub mod skills;
pub mod subagent_coordinator;
pub mod task_plan;
pub mod tool_audit;
pub mod tool_catalog;
pub mod tool_dispatch;
pub mod tool_execution_pipeline;
pub mod tool_executor;
pub mod tool_fallback;
pub mod tool_policy;
pub mod trace;
pub mod web_evidence_broker;
pub mod writing_state;

// 閳光偓閳光偓閳光偓 Re-exports from ai_workflows (backward compatibility) 閳光偓
pub use crate::ai_workflows::assistant_facade;
pub use crate::ai_workflows::chapter_workflow;
pub use crate::ai_workflows::citation_workflow;
pub use crate::ai_workflows::document_workflow;
pub use crate::ai_workflows::organize_workflow;
pub use crate::ai_workflows::research_workflow;
pub use crate::ai_workflows::writing_workflow;

// 閳光偓閳光偓閳光偓 Re-exports from ai_harness (backward compatibility) 閳光偓
pub use crate::ai_harness::evidence_ledger;
pub use crate::ai_harness::evidence_mixer;
pub use crate::ai_harness::harness;
pub use crate::ai_harness::harness_confirm;
pub use crate::ai_harness::harness_support;
pub use crate::ai_harness::harness_task;

use serde::{Deserialize, Serialize};

// 閳光偓閳光偓閳光偓 AssembledContext (kept here: depends on execution_plan::ExecutionPlanDto) 閳光偓

/// 缂佸嫯顥婇崥搴ｆ畱娑撳﹣绗呴弬鍥风礉閸栧懎鎯堢拠浣瑰祦閸栧懌鈧礁褰查悽銊ヤ紣閸忓嘲鎷伴悩鑸碘偓浣规喅鐟曚降鈧?///
/// 閻?context_planner 缂佸嫯顥婇敍宀€娲块幒銉ょ炊閸?model_gateway 閺嬪嫬缂?prompt閵?
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssembledContext {
    #[serde(default)]
    pub provisional: bool,
    pub packets: Vec<ContextPacket>,
    pub tools: Vec<ToolSpec>,
    pub context_status: ContextStatus,
    pub execution_plan: Option<crate::ai_runtime::execution_plan::ExecutionPlanDto>,
}
