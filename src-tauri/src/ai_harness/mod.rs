//! AI harness — the agentic execution loop, tool dispatch, evidence
//! management, and multi-round orchestration infrastructure.

pub mod evidence_ledger;
pub mod evidence_mixer;
pub mod harness;
pub mod harness_confirm;
#[cfg(test)]
mod harness_confirm_tests;
pub mod harness_support;
pub mod harness_task;
pub mod tool_turn;
