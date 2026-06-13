//! Iris AI Runtime — scene routing, workflows, tool permission, trace.
//!
//! Shared data types live in [`crate::ai_types`] and are re-exported here
//! for backward compatibility.

pub use crate::ai_types::*;

// ─── Modules that remain in ai_runtime (coordination layer) ─
pub mod agent_permissions;
pub mod context_cache;
pub mod context_planner;
pub mod environment;
pub mod execution_plan;
pub mod guardrails;
pub mod model_gateway;
pub mod model_registry;
pub mod packet_builder;
pub mod packet_cache;
pub mod persona_resolver;
pub mod prompt_builder;
pub mod prompt_profile;
pub mod retrieval_broker;
pub mod retrieval_scope;
pub mod scene_router;
pub mod session;
pub mod skill_install_service;
pub mod skill_registry;
pub mod skills;
pub mod tool_audit;
pub mod tool_catalog;
pub mod tool_dispatch;
pub mod tool_executor;
pub mod tool_fallback;
pub mod tool_policy;
pub mod trace;

// ─── Re-exports from ai_workflows (backward compatibility) ─
pub use crate::ai_workflows::assistant_facade;
pub use crate::ai_workflows::chapter_workflow;
pub use crate::ai_workflows::citation_workflow;
pub use crate::ai_workflows::document_workflow;
pub use crate::ai_workflows::organize_workflow;
pub use crate::ai_workflows::research_workflow;
pub use crate::ai_workflows::writing_workflow;

// ─── Re-exports from ai_harness (backward compatibility) ─
pub use crate::ai_harness::evidence_ledger;
pub use crate::ai_harness::evidence_mixer;
pub use crate::ai_harness::harness;
pub use crate::ai_harness::harness_confirm;
pub use crate::ai_harness::harness_support;
pub use crate::ai_harness::harness_task;

use serde::{Deserialize, Serialize};

// ─── AssembledContext (kept here: depends on execution_plan::ExecutionPlanDto) ─

/// 组装后的上下文，包含证据包、可用工具和状态摘要。
///
/// 由 context_planner 组装，直接传入 model_gateway 构建 prompt。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssembledContext {
    #[serde(default)]
    pub provisional: bool,
    pub packets: Vec<ContextPacket>,
    pub tools: Vec<ToolSpec>,
    pub context_status: ContextStatus,
    pub execution_plan: Option<crate::ai_runtime::execution_plan::ExecutionPlanDto>,
}
