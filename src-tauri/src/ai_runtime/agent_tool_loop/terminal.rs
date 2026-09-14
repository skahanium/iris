//! Explicit terminal identity for one bounded model/tool exchange.
//!
//! The Host identity of a terminal answer is a decision the loop already made.
//! Recording it as a type keeps the finalisation layer from re-deriving that
//! decision from the visible text, which used to let model prose impersonate a
//! Host-authored limitation (and thereby skip validation) merely by starting
//! with the limitation's own opening words.

use crate::ai_runtime::final_answer_submission::FinalAnswerSubmission;

/// Which producer owns the terminal assistant body of one bounded Run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentTerminalType {
    /// Ordinary model-authored content. It stays subject to every final-answer,
    /// finish-reason and source-binding check.
    ModelAnswer,
    /// Ordinary model-authored content the Host repaired or forced into a
    /// synthesis turn. It is still model prose and therefore validated exactly
    /// like `ModelAnswer`; the variant only records how the turn was reached.
    RepairedModelAnswer,
    /// The Host closed the Run with its own bounded limitation because the
    /// frozen evidence or projection contract could not be satisfied. This body
    /// is not model output and is never cited.
    HostEvidenceLimited,
}

impl AgentTerminalType {
    /// Whether the body came from the Host rather than the Provider.
    ///
    /// Only this classification may relax Provider-output validation. It is
    /// derived from the loop's own control flow and never from answer text.
    pub(crate) const fn is_host_authored(self) -> bool {
        matches!(self, Self::HostEvidenceLimited)
    }

    /// Stable bounded diagnostic label for the classification itself.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ModelAnswer => "model_answer",
            Self::RepairedModelAnswer => "repaired_model_answer",
            Self::HostEvidenceLimited => "host_evidence_limited",
        }
    }
}

/// Result of a fully bounded model/tool exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentToolLoopOutcome {
    /// Final assistant content emitted only after the model has stopped calling tools.
    pub(crate) content: String,
    /// Explicit producer identity for `content`. Validation, citation binding
    /// and evidence commit all consume this instead of inspecting the text.
    pub(crate) terminal: AgentTerminalType,
    /// Provider stop reason associated with the final assistant content. It
    /// expresses only why the Provider stopped and never carries Host identity;
    /// a Host-authored limitation leaves it as `"stop"` because no Provider
    /// stop caused that terminal.
    pub(crate) finish_reason: String,
    /// Internal structured submission when the model used the reserved final
    /// answer tool. It never enters the model transcript or tool audit.
    pub(crate) final_submission: Option<FinalAnswerSubmission>,
    /// Number of model turns used by this Run.
    pub(crate) model_turns: u32,
    /// Number of concrete tool dispatch attempts made by this Run.
    pub(crate) tool_calls: u32,
    /// Provider-reported input tokens consumed across all model turns.
    pub(crate) prompt_tokens: u32,
    /// Provider-reported output tokens consumed across all model turns.
    pub(crate) completion_tokens: u32,
    /// Provider-reported total tokens consumed across all model turns.
    pub(crate) total_tokens: u32,
}

/// Classify Provider prose by how the Host reached the publishing turn. Both
/// variants stay subject to the same Provider-output validation; the split only
/// records whether the Host had to withhold a draft first.
pub(super) const fn model_terminal_type(repaired: bool) -> AgentTerminalType {
    if repaired {
        AgentTerminalType::RepairedModelAnswer
    } else {
        AgentTerminalType::ModelAnswer
    }
}

/// Build the Host-authored bounded limitation terminal.
#[allow(clippy::too_many_arguments)]
pub(crate) fn evidence_limited_outcome(
    content: String,
    model_turns: u32,
    tool_calls: u32,
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
) -> AgentToolLoopOutcome {
    AgentToolLoopOutcome {
        content,
        terminal: AgentTerminalType::HostEvidenceLimited,
        // No Provider stop produced this terminal. Carrying a synthetic reason
        // here is what previously forced the finalisation layer to sniff the
        // body text to tell Host copy from model prose.
        finish_reason: "stop".to_string(),
        final_submission: None,
        model_turns,
        tool_calls,
        prompt_tokens,
        completion_tokens,
        total_tokens,
    }
}

pub(crate) const EVIDENCE_LIMITED_RESPONSE: &str =
    "本轮未取得足够的可核验来源正文，无法确认问题涉及的当前情况。未核实的线索不能作为当前事实的依据。";
