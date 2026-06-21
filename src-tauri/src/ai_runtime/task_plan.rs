//! Server-side TaskPlan validation and compatibility derivation.

use crate::ai_runtime::agent_task_policy::{AgentTaskPolicy, AgentTaskPolicyInput, AgentTaskScope};
use crate::ai_runtime::assistant_facade::AssistantIntent;
use crate::ai_runtime::AgentIntent;
use crate::ai_types::{
    CapabilitySlot, ExecutionMode, OutputMode, RetrievalMode, TaskPlanConfidence, TaskPlanIntent,
    TaskPlanSummary, WebMode,
};
use crate::commands::assistant_commands::AssistantExecuteRequest;
use crate::error::{AppError, AppResult};

/// Build a compatibility TaskPlan or validate the frontend-provided one.
pub fn build_or_validate_task_plan(
    request: &AssistantExecuteRequest,
) -> AppResult<TaskPlanSummary> {
    let mut plan = request
        .task_plan
        .clone()
        .unwrap_or_else(|| derive_compat_task_plan(request));

    if plan.context_references.is_empty() && !request.context_references.is_empty() {
        plan.context_references = request.context_references.clone();
    }
    if !plan.context_references.is_empty() {
        plan.retrieval_mode = RetrievalMode::CurrentReference;
    } else if request.context_scope.is_some() {
        plan.retrieval_mode = RetrievalMode::ScopedNotes;
    } else if matches!(plan.intent, TaskPlanIntent::DocumentCheck)
        || matches!(plan.retrieval_mode, RetrievalMode::LongDocument)
    {
        plan.retrieval_mode = RetrievalMode::LongDocument;
    } else if matches!(
        plan.intent,
        TaskPlanIntent::AskNotes | TaskPlanIntent::Research
    ) {
        plan.retrieval_mode = RetrievalMode::LocalNotes;
    }

    if !request.web_authorized || matches!(plan.web_mode, WebMode::Disabled) {
        plan.web_mode = WebMode::Disabled;
    } else {
        plan.web_mode = WebMode::Brokered;
    }

    plan.execution_mode = execution_mode_for_task_plan(&plan);
    if matches!(plan.execution_mode, ExecutionMode::Clarification) {
        plan.requires_clarification = true;
    }
    if plan.requires_clarification
        && plan
            .clarification_question
            .as_deref()
            .map(str::is_empty)
            .unwrap_or(true)
    {
        return Err(AppError::msg(
            "TaskPlan requires clarification but has no clarificationQuestion",
        ));
    }
    plan.output_mode = output_mode_for_task_plan(&plan);
    let policy = AgentTaskPolicy::from_input(AgentTaskPolicyInput::from_task_plan(&plan, request));
    plan.model_slot = policy.model_slot;

    Ok(plan)
}

/// Convert TaskPlan intent into the Phase 2 agent intent used by runtime policy.
pub fn agent_intent_for_task_plan(plan: &TaskPlanSummary) -> AgentIntent {
    match plan.intent {
        TaskPlanIntent::Chat => AgentIntent::Chat,
        TaskPlanIntent::AskNotes => AgentIntent::AskNotes,
        TaskPlanIntent::CreativeWrite => AgentIntent::Write,
        TaskPlanIntent::RewriteSelection => AgentIntent::RewriteSelection,
        TaskPlanIntent::CitationCheck => AgentIntent::CitationCheck,
        TaskPlanIntent::Research => AgentIntent::Research,
        TaskPlanIntent::Organize => AgentIntent::Organize,
        TaskPlanIntent::DocumentCheck => AgentIntent::DocumentCheck,
        TaskPlanIntent::Chapter => AgentIntent::Chapter,
        TaskPlanIntent::VisionChat => AgentIntent::VisionChat,
        TaskPlanIntent::SkillManagement => AgentIntent::SkillManagement,
    }
}

/// Build the task-policy input from a validated TaskPlan.
pub fn policy_input_for_task_plan(
    plan: &TaskPlanSummary,
    request: &AssistantExecuteRequest,
) -> AgentTaskPolicyInput {
    AgentTaskPolicyInput::from_task_plan(plan, request)
}

/// Convert TaskPlan intent to the legacy workflow intent during migration.
pub fn legacy_intent_for_task_plan(plan: &TaskPlanSummary) -> AssistantIntent {
    match plan.intent {
        TaskPlanIntent::Chat | TaskPlanIntent::VisionChat | TaskPlanIntent::SkillManagement => {
            AssistantIntent::Chat
        }
        TaskPlanIntent::AskNotes => AssistantIntent::Knowledge,
        TaskPlanIntent::CreativeWrite | TaskPlanIntent::RewriteSelection => {
            AssistantIntent::Writing
        }
        TaskPlanIntent::CitationCheck => AssistantIntent::Citation,
        TaskPlanIntent::Research => AssistantIntent::Research,
        TaskPlanIntent::Organize => AssistantIntent::Organize,
        TaskPlanIntent::DocumentCheck => AssistantIntent::Document,
        TaskPlanIntent::Chapter => AssistantIntent::Chapter,
    }
}

pub(crate) fn task_kind_for_task_plan(
    plan: &TaskPlanSummary,
) -> crate::ai_runtime::agent_task::AgentTaskKind {
    if matches!(
        plan.intent,
        TaskPlanIntent::Research
            | TaskPlanIntent::CitationCheck
            | TaskPlanIntent::DocumentCheck
            | TaskPlanIntent::Chapter
    ) || matches!(
        plan.execution_mode,
        ExecutionMode::StructuredTask | ExecutionMode::LongTask
    ) {
        crate::ai_runtime::agent_task::AgentTaskKind::Complex
    } else {
        crate::ai_runtime::agent_task::AgentTaskKind::Lightweight
    }
}

pub(crate) fn scope_for_task_plan(
    plan: &TaskPlanSummary,
    request: &AssistantExecuteRequest,
) -> AgentTaskScope {
    if !plan.context_references.is_empty()
        || request
            .selection
            .as_ref()
            .is_some_and(|s| !s.trim().is_empty())
    {
        AgentTaskScope::Selection
    } else if request.note_path.is_some() {
        AgentTaskScope::Note
    } else {
        AgentTaskScope::Vault
    }
}

pub(crate) fn write_permission_required_for_task_plan(plan: &TaskPlanSummary) -> bool {
    matches!(
        plan.intent,
        TaskPlanIntent::CreativeWrite
            | TaskPlanIntent::RewriteSelection
            | TaskPlanIntent::Chapter
            | TaskPlanIntent::DocumentCheck
    )
}

pub(crate) fn research_depth_for_task_plan(plan: &TaskPlanSummary) -> u32 {
    if matches!(
        plan.intent,
        TaskPlanIntent::Research | TaskPlanIntent::CitationCheck
    ) {
        if matches!(plan.execution_mode, ExecutionMode::LongTask) {
            2
        } else {
            1
        }
    } else {
        0
    }
}

fn derive_compat_task_plan(request: &AssistantExecuteRequest) -> TaskPlanSummary {
    let intent = if request
        .images
        .as_ref()
        .is_some_and(|items| !items.is_empty())
    {
        TaskPlanIntent::VisionChat
    } else if request
        .selection
        .as_ref()
        .is_some_and(|selection| !selection.trim().is_empty())
        && request.agent_intent.is_some_and(|intent| {
            matches!(intent, AgentIntent::RewriteSelection | AgentIntent::Write)
        })
    {
        TaskPlanIntent::RewriteSelection
    } else if let Some(agent_intent) = request.agent_intent {
        task_plan_intent_for_agent_intent(agent_intent)
    } else {
        TaskPlanIntent::Chat
    };

    let mut source_hints = request
        .intent_detection
        .as_ref()
        .map(|summary| summary.source_hints.clone())
        .unwrap_or_default();
    push_unique(&mut source_hints, "compat:server_derived_task_plan");

    TaskPlanSummary {
        intent,
        confidence: TaskPlanConfidence::Medium,
        context_references: request.context_references.clone(),
        retrieval_mode: RetrievalMode::None,
        web_mode: if request.web_authorized {
            WebMode::Brokered
        } else {
            WebMode::Disabled
        },
        model_slot: CapabilitySlot::Fast,
        execution_mode: ExecutionMode::DirectAnswer,
        output_mode: OutputMode::MarkdownMessage,
        artifact_plan: Vec::new(),
        requires_clarification: false,
        clarification_question: None,
        source_hints,
    }
}

fn task_plan_intent_for_agent_intent(intent: AgentIntent) -> TaskPlanIntent {
    match intent {
        AgentIntent::Chat => TaskPlanIntent::Chat,
        AgentIntent::AskNotes => TaskPlanIntent::AskNotes,
        AgentIntent::RewriteSelection => TaskPlanIntent::RewriteSelection,
        AgentIntent::Write => TaskPlanIntent::CreativeWrite,
        AgentIntent::Research => TaskPlanIntent::Research,
        AgentIntent::Organize => TaskPlanIntent::Organize,
        AgentIntent::CitationCheck => TaskPlanIntent::CitationCheck,
        AgentIntent::Chapter => TaskPlanIntent::Chapter,
        AgentIntent::DocumentCheck => TaskPlanIntent::DocumentCheck,
        AgentIntent::VisionChat => TaskPlanIntent::VisionChat,
        AgentIntent::SkillManagement => TaskPlanIntent::SkillManagement,
    }
}

fn execution_mode_for_task_plan(plan: &TaskPlanSummary) -> ExecutionMode {
    if plan.requires_clarification {
        return ExecutionMode::Clarification;
    }
    match plan.intent {
        TaskPlanIntent::CreativeWrite => ExecutionMode::WritingCandidate,
        TaskPlanIntent::RewriteSelection => ExecutionMode::PatchProposal,
        TaskPlanIntent::Research | TaskPlanIntent::CitationCheck
            if !matches!(
                plan.execution_mode,
                ExecutionMode::StructuredTask | ExecutionMode::LongTask
            ) =>
        {
            ExecutionMode::StructuredTask
        }
        TaskPlanIntent::DocumentCheck | TaskPlanIntent::Chapter => ExecutionMode::LongTask,
        TaskPlanIntent::AskNotes
            if matches!(plan.retrieval_mode, RetrievalMode::CurrentReference)
                || matches!(
                    plan.retrieval_mode,
                    RetrievalMode::LocalNotes | RetrievalMode::ScopedNotes
                ) =>
        {
            ExecutionMode::ContextAnswer
        }
        _ => plan.execution_mode,
    }
}

fn output_mode_for_task_plan(plan: &TaskPlanSummary) -> OutputMode {
    match plan.execution_mode {
        ExecutionMode::PatchProposal => OutputMode::ConfirmationRequired,
        ExecutionMode::StructuredTask | ExecutionMode::LongTask
            if !plan.artifact_plan.is_empty() =>
        {
            OutputMode::ArtifactBackedMessage
        }
        ExecutionMode::Clarification => OutputMode::Diagnostic,
        _ => plan.output_mode,
    }
}

fn push_unique(hints: &mut Vec<String>, hint: &str) {
    if !hints.iter().any(|item| item == hint) {
        hints.push(hint.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::agent_task_policy::AgentTaskPolicy;
    use crate::ai_types::{
        CapabilitySlot, ExecutionMode, RetrievalMode, TaskPlanConfidence, TaskPlanIntent,
        TaskPlanSummary, WebMode,
    };

    fn request_with_plan(plan: TaskPlanSummary) -> AssistantExecuteRequest {
        AssistantExecuteRequest {
            agent_intent: None,
            intent: None,
            intent_detection: None,
            task_plan: Some(plan),
            context_references: Vec::new(),
            message: "hello".into(),
            note_path: None,
            note_content: None,
            web_authorized: false,
            selection: None,
            cursor_context: None,
            paragraph_text: None,
            context_scope: None,
            session_id: None,
            selected_packet_ids: None,
            chapter: None,
            document_check_type: None,
            organize_task_type: None,
            base_content_hash: None,
            new_session: false,
            images: None,
        }
    }

    fn plan(intent: TaskPlanIntent) -> TaskPlanSummary {
        TaskPlanSummary {
            intent,
            confidence: TaskPlanConfidence::High,
            context_references: Vec::new(),
            retrieval_mode: RetrievalMode::None,
            web_mode: WebMode::Disabled,
            model_slot: CapabilitySlot::Fast,
            execution_mode: ExecutionMode::DirectAnswer,
            output_mode: crate::ai_types::OutputMode::MarkdownMessage,
            artifact_plan: Vec::new(),
            requires_clarification: false,
            clarification_question: None,
            source_hints: Vec::new(),
        }
    }

    #[test]
    fn creative_write_uses_writer_slot_even_with_analysis_word() {
        let mut request = request_with_plan(plan(TaskPlanIntent::CreativeWrite));
        request.message = "继续写下一章，同时分析人物心理".into();

        let task_plan = build_or_validate_task_plan(&request).unwrap();

        assert_eq!(task_plan.model_slot, CapabilitySlot::Writer);
        assert_eq!(task_plan.execution_mode, ExecutionMode::WritingCandidate);
        assert_eq!(agent_intent_for_task_plan(&task_plan), AgentIntent::Write);
    }

    #[test]
    fn chat_uses_fast_slot_without_retrieval() {
        let task_plan =
            build_or_validate_task_plan(&request_with_plan(plan(TaskPlanIntent::Chat))).unwrap();

        assert_eq!(task_plan.model_slot, CapabilitySlot::Fast);
        assert_eq!(task_plan.retrieval_mode, RetrievalMode::None);
    }

    #[test]
    fn research_requires_structured_task_or_long_task() {
        let task_plan =
            build_or_validate_task_plan(&request_with_plan(plan(TaskPlanIntent::Research)))
                .unwrap();

        assert_eq!(task_plan.model_slot, CapabilitySlot::Reasoner);
        assert!(matches!(
            task_plan.execution_mode,
            ExecutionMode::StructuredTask | ExecutionMode::LongTask
        ));
    }

    #[test]
    fn context_reference_sets_current_reference_retrieval() {
        let mut task_plan = plan(TaskPlanIntent::AskNotes);
        task_plan
            .context_references
            .push(crate::ai_types::ContextReferenceWire {
                id: "ref-1".into(),
                kind: crate::ai_types::ContextReferenceKind::Selection,
                file_path: Some("/note.md".into()),
                content_hash: Some("hash".into()),
                utf8_range: None,
                editor_range: None,
                excerpt: "selected text".into(),
                heading_path: None,
                anchor: None,
                stale: false,
                invalid_reason: None,
            });

        let task_plan = build_or_validate_task_plan(&request_with_plan(task_plan)).unwrap();

        assert_eq!(task_plan.retrieval_mode, RetrievalMode::CurrentReference);
    }

    #[test]
    fn web_disabled_sets_max_fetch_zero() {
        let mut request = request_with_plan(plan(TaskPlanIntent::Research));
        request.web_authorized = true;

        let task_plan = build_or_validate_task_plan(&request).unwrap();
        let input = policy_input_for_task_plan(&task_plan, &request);
        let policy = AgentTaskPolicy::from_input(input);

        assert!(!input.web_authorized);
        assert_eq!(policy.max_fetch_per_round, 0);
    }

    #[test]
    fn clarification_requires_question() {
        let mut task_plan = plan(TaskPlanIntent::Chat);
        task_plan.requires_clarification = true;
        task_plan.execution_mode = ExecutionMode::Clarification;

        let result = build_or_validate_task_plan(&request_with_plan(task_plan));

        assert!(result.is_err());
    }

    #[test]
    fn returned_model_slot_matches_policy_model_slot() {
        let mut task_plan = plan(TaskPlanIntent::Research);
        task_plan.execution_mode = ExecutionMode::LongTask;
        let request = request_with_plan(task_plan);

        let task_plan = build_or_validate_task_plan(&request).unwrap();
        let policy = AgentTaskPolicy::from_input(policy_input_for_task_plan(&task_plan, &request));

        assert_eq!(task_plan.model_slot, policy.model_slot);
    }
}
