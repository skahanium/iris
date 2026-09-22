//! C25 delivery expression: K15 task outcome derived from terminal facts.
//!
//! `completed` / `partial` / `blocked` are task results, not Run lifecycle.
//! Classification reads Host terminal identity and write receipts only — never
//! the assistant body, and never a `capability_degraded` event. The value is
//! attached to `RunEventPayload::Completed` JSON; no SQLite column is added.

use serde::{Deserialize, Serialize};

/// User-visible task result published with a Completed Run event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TaskOutcome {
    Completed,
    Partial,
    Blocked,
}

/// Facts the Host already holds at publication time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeliveryFacts {
    /// `AgentTerminalType::HostEvidenceLimited` closed this Run.
    pub(crate) host_authored_limitation: bool,
    /// `Some(false)` when a confirmed change set did not dispatch every op.
    pub(crate) change_ops_complete: Option<bool>,
}

/// Derive the K15 task result from Host facts without inspecting answer text.
pub(crate) fn classify_task_outcome(facts: &DeliveryFacts) -> TaskOutcome {
    if facts.host_authored_limitation {
        TaskOutcome::Blocked
    } else if facts.change_ops_complete == Some(false) {
        TaskOutcome::Partial
    } else {
        TaskOutcome::Completed
    }
}
