//! Iris unified Run runtime.
//!
//! Shared data types live in [`crate::ai_types`] and are re-exported here
//! for backward compatibility.

pub use crate::ai_types::*;

#[allow(dead_code)]
pub(crate) mod agent_evidence_repository;
#[cfg(test)]
mod agent_evidence_repository_tests;
pub mod agent_permissions;
#[allow(dead_code)]
pub(crate) mod agent_run_repository;
#[cfg(test)]
mod agent_run_repository_tests;
pub(crate) mod agent_tool_loop;
#[cfg(test)]
mod agent_tool_loop_tests;
pub mod capability_resolver;
pub mod circuit_breaker;
pub(crate) mod classified_document_policy_repository;
pub mod classified_retrieval;
pub(crate) mod classified_run_engine;
pub mod classified_session;
pub mod context_cache;
pub mod conversation_memory;
#[allow(dead_code)]
pub(crate) mod direct_provider_route;
pub(crate) mod document_policy_repository;
pub(crate) mod domain_executor;
#[cfg(test)]
mod domain_executor_tests;
#[allow(dead_code)]
pub(crate) mod frozen_change_plan;
#[cfg(test)]
mod frozen_change_plan_tests;
pub mod guardrails;
pub mod mcp_host_runtime;
pub mod mcp_runtime_registry;
pub mod model_gateway;
pub mod model_registry;
#[allow(dead_code)]
pub(crate) mod normal_session_repository;
#[cfg(test)]
mod normal_session_repository_tests;
pub mod permission_decision;
#[allow(dead_code)]
pub(crate) mod policy_decision_engine;
pub mod prompt_profile;
#[allow(dead_code)]
pub(crate) mod provider_router;
pub mod retrieval_broker;
pub mod retrieval_scope;
pub(crate) mod run_context;
#[cfg(test)]
mod run_context_tests;
#[allow(dead_code)]
pub(crate) mod run_contract;
#[cfg(test)]
mod run_contract_tests;
#[allow(dead_code)]
pub(crate) mod run_engine;
#[cfg(test)]
mod run_engine_tests;
#[allow(dead_code)]
pub(crate) mod run_intake;
#[cfg(test)]
mod run_intake_tests;
pub(crate) mod run_tool_loop;
pub mod runtime_context;
pub mod sandbox_profile;
pub mod skills;
pub mod subagent_coordinator;
pub(crate) mod text_support;
pub mod tool_audit;
pub mod tool_catalog;
pub mod tool_dispatch;
pub mod tool_effects;
pub mod tool_execution_pipeline;
pub mod tool_executor;
pub mod tool_fallback;
pub mod tool_policy;
pub mod trace;
pub mod web_evidence_broker;
pub mod writing_state;
