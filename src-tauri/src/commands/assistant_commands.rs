//! Unified assistant IPC facade — routes intents to existing workflows.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::ai_runtime::assistant_facade::{
    agent_intent_from_legacy, legacy_intent_for_agent, AssistantIntent,
};
use crate::ai_runtime::chapter_workflow::ChapterInfo;
use crate::ai_runtime::document_workflow::DocumentCheckResult;
use crate::ai_runtime::retrieval_scope::ContextScopeDto;
use crate::ai_runtime::task_plan::{
    agent_intent_for_task_plan, build_or_validate_task_plan, legacy_intent_for_task_plan,
};
use crate::ai_runtime::writing_workflow::WritingTaskOutput;
use crate::ai_runtime::{
    AgentAuditSummary, AgentIntent, AgentRunPlanSummary, CitationCheckResult, ContextReferenceWire,
    ExecutionMode, IntentDetectionSummary, OrganizeTaskResult, PermissionPreflightSummary,
    RuntimeDocumentSnapshot, SkillActivationPlanSummary, SkillCapabilitySupportStatus,
    TaskPlanSummary,
};
use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::storage::paths::is_classified_note_path;

/// Assistant data domain. Normal requests may only use ordinary notes; classified
/// requests may only use `.classified/` notes and stay out of normal sessions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssistantAiDomain {
    #[default]
    Normal,
    Classified,
}

/// Unified assistant execution request (camelCase for TypeScript IPC).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantExecuteRequest {
    #[serde(default)]
    pub ai_domain: AssistantAiDomain,
    #[serde(default)]
    pub agent_intent: Option<AgentIntent>,
    #[serde(default)]
    pub intent: Option<AssistantIntent>,
    #[serde(default)]
    pub intent_detection: Option<IntentDetectionSummary>,
    #[serde(default)]
    pub task_plan: Option<TaskPlanSummary>,
    #[serde(default)]
    pub context_references: Vec<ContextReferenceWire>,
    #[serde(default)]
    pub runtime_documents: Vec<RuntimeDocumentSnapshot>,
    pub message: String,
    #[serde(default)]
    pub note_path: Option<String>,
    #[serde(default)]
    pub note_content: Option<String>,
    #[serde(default)]
    pub web_authorized: bool,
    #[serde(default)]
    pub selection: Option<String>,
    #[serde(default)]
    pub cursor_context: Option<String>,
    #[serde(default)]
    pub paragraph_text: Option<String>,
    #[serde(default)]
    pub context_scope: Option<ContextScopeDto>,
    #[serde(default)]
    pub session_id: Option<i64>,
    #[serde(default)]
    pub selected_packet_ids: Option<Vec<String>>,
    #[serde(default)]
    pub chapter: Option<ChapterInfo>,
    #[serde(default)]
    pub document_check_type: Option<String>,
    #[serde(default)]
    pub organize_task_type: Option<String>,
    #[serde(default)]
    pub base_content_hash: Option<String>,
    /// 为 true 时创建新的 session 线程，不续接同 scene+笔记 的历史消息。
    #[serde(default)]
    pub new_session: bool,
    /// 图片附件（多模态消息）。
    #[serde(default)]
    pub images: Option<Vec<crate::commands::ai_commands::ImageAttachmentDto>>,
}

impl AssistantExecuteRequest {
    pub fn effective_agent_intent(&self) -> AgentIntent {
        self.agent_intent.unwrap_or_else(|| {
            agent_intent_from_legacy(
                self.intent.unwrap_or(AssistantIntent::Chat),
                Some(
                    self.selection
                        .as_ref()
                        .is_some_and(|s| !s.trim().is_empty()),
                ),
            )
        })
    }

    pub fn effective_legacy_intent(&self) -> AssistantIntent {
        self.intent
            .unwrap_or_else(|| legacy_intent_for_agent(self.effective_agent_intent()))
    }

    fn detection_summary(&self) -> IntentDetectionSummary {
        self.intent_detection
            .clone()
            .unwrap_or_else(|| IntentDetectionSummary {
                detected_intent: self.effective_agent_intent(),
                confidence: if self.agent_intent.is_some() {
                    0.9
                } else {
                    0.72
                },
                reason: "Derived from assistant_execute request metadata.".into(),
                alternatives: Vec::new(),
                fallback_behavior:
                    "Use the compatible legacy workflow if the Phase2 route is unavailable.".into(),
                source_hints: self.source_hints(),
            })
    }

    fn source_hints(&self) -> Vec<String> {
        let mut hints = Vec::new();
        if self.note_path.is_some() {
            hints.push("context:note".to_string());
        }
        if self
            .selection
            .as_ref()
            .is_some_and(|s| !s.trim().is_empty())
        {
            hints.push("context:selection".to_string());
        }
        if self.context_scope.is_some() {
            hints.push("context:scope".to_string());
        }
        hints
    }

    fn context_summary(&self) -> Vec<String> {
        let mut summary = Vec::new();
        if let Some(path) = &self.note_path {
            summary.push(format!("当前笔记：{path}"));
        }
        if self
            .selection
            .as_ref()
            .is_some_and(|s| !s.trim().is_empty())
        {
            summary.push("包含选中文本摘要".to_string());
        }
        if self.context_scope.is_some() {
            summary.push("包含用户指定检索范围".to_string());
        }
        if !self.context_references.is_empty() {
            summary.push(format!(
                "包含 {} 个上下文引用",
                self.context_references.len()
            ));
        }
        if !self.runtime_documents.is_empty() {
            summary.push(format!(
                "包含 {} 个运行期文档快照",
                self.runtime_documents.len()
            ));
        }
        if self.task_plan.is_some() {
            summary.push("包含 TaskPlan 摘要".to_string());
        }
        if self.web_authorized {
            summary.push("允许联网检索".to_string());
        }
        if summary.is_empty() {
            summary.push("无额外上下文".to_string());
        }
        summary
    }
}

fn validate_note_content_boundary(request: &AssistantExecuteRequest) -> AppResult<()> {
    if request.ai_domain == AssistantAiDomain::Classified
        && request
            .note_content
            .as_ref()
            .is_some_and(|content| !content.trim().is_empty())
    {
        return Err(AppError::msg(
            "classified assistant requests must not carry noteContent",
        ));
    }
    if request.note_path.is_none()
        && request
            .note_content
            .as_ref()
            .is_some_and(|content| !content.trim().is_empty())
    {
        return Err(AppError::msg(
            "assistant_execute noteContent requires a non-classified notePath",
        ));
    }
    Ok(())
}

pub(crate) fn validate_assistant_domain_boundary(
    request: &AssistantExecuteRequest,
) -> AppResult<()> {
    match request.ai_domain {
        AssistantAiDomain::Normal => {
            crate::commands::ai_commands::validate_ai_note_path(request.note_path.as_deref())?;
        }
        AssistantAiDomain::Classified => {
            let note_path = request
                .note_path
                .as_deref()
                .ok_or_else(|| AppError::msg("classified assistant requires notePath"))?;
            if !is_classified_note_path(note_path) {
                return Err(AppError::msg(
                    "classified assistant requires a .classified notePath",
                ));
            }
            if !request.context_references.is_empty() {
                return Err(AppError::msg(
                    "classified assistant cannot use normal context references",
                ));
            }
            if !request.runtime_documents.is_empty() {
                return Err(AppError::msg(
                    "classified assistant cannot use normal runtime documents",
                ));
            }
        }
    }
    validate_note_content_boundary(request)
}

fn permission_summary_for_status(run_status: &str) -> String {
    match run_status {
        "pending_confirmation" => {
            "权限预检发现需要用户确认的工具或写入步骤；本轮已暂停等待决定。".to_string()
        }
        "failed" => "权限或执行链路未能完成；请查看阻断原因与审计信息。".to_string(),
        "aborted" => "本轮执行已中止；不会继续调用工具或写入内容。".to_string(),
        _ => "权限预检通过当前 ToolPolicy；写入和工具确认仍会进入统一确认。".to_string(),
    }
}

fn build_permission_preflight_summary(
    plan: &SkillActivationPlanSummary,
    run_status: &str,
) -> PermissionPreflightSummary {
    let mut missing_user_grants = Vec::new();
    for blocked in &plan.blocked_capabilities {
        if blocked.status == SkillCapabilitySupportStatus::MissingUserGrant
            && !missing_user_grants.contains(&blocked.capability)
        {
            missing_user_grants.push(blocked.capability.clone());
        }
    }
    PermissionPreflightSummary {
        summary: permission_summary_for_status(run_status),
        required_confirmations: plan.confirmation_required_tools.clone(),
        blocked_capabilities: plan.blocked_capabilities.clone(),
        missing_user_grants,
        exposed_tools: plan.requested_tools.clone(),
        degraded: plan.degraded || run_status != "completed",
    }
}

fn blocked_reasons_for_response(response: &AssistantExecuteResponse) -> Vec<String> {
    if response.run_status == "pending_confirmation" {
        let tool_titles: Vec<String> = response
            .artifacts
            .iter()
            .filter(|artifact| is_pending_confirmation_artifact(artifact))
            .map(|artifact| format!("等待确认：{}", artifact.title))
            .collect();
        if tool_titles.is_empty() {
            vec!["等待用户确认工具或写入步骤".to_string()]
        } else {
            tool_titles
        }
    } else if response.run_status == "failed" {
        vec!["Harness 返回失败状态；检查错误消息或审计记录".to_string()]
    } else if response.run_status == "aborted" {
        vec!["用户或运行时中止了本轮任务".to_string()]
    } else {
        Vec::new()
    }
}

fn is_pending_confirmation_artifact(
    artifact: &crate::ai_runtime::harness_task::HarnessArtifactWire,
) -> bool {
    artifact.kind == "task_process"
        && artifact
            .payload
            .get("status")
            .and_then(serde_json::Value::as_str)
            == Some("pending_confirmation")
}

/// Task body returned to the frontend (tagged union, wire-compatible).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AssistantExecuteBody {
    Chat {
        payload: serde_json::Value,
    },
    Writing {
        payload: WritingTaskOutput,
    },
    Citation {
        payload: CitationCheckResult,
    },
    Organize {
        payload: OrganizeTaskResult,
    },
    Research {
        payload: serde_json::Value,
    },
    Chapter {
        payload: crate::ai_runtime::chapter_workflow::ChapterWritingResult,
    },
    Document {
        payload: DocumentCheckResult,
    },
}

/// Unified harness metadata + flattened task body for IPC.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantExecuteResponse {
    #[serde(flatten)]
    pub body: AssistantExecuteBody,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub run_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_refresh_notice: Option<String>,
    pub artifacts: Vec<crate::ai_runtime::harness_task::HarnessArtifactWire>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent_detection: Option<IntentDetectionSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_plan: Option<TaskPlanSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_plan_summary: Option<AgentRunPlanSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_preflight_summary: Option<PermissionPreflightSummary>,
}

fn clarification_response(
    task_plan: TaskPlanSummary,
    intent_detection: IntentDetectionSummary,
    legacy_scene_hint: crate::ai_runtime::AiScene,
    context_summary: Vec<String>,
) -> AssistantExecuteResponse {
    let request_id = uuid::Uuid::new_v4().to_string();
    let detected_intent = intent_detection.detected_intent;
    let question = task_plan
        .clarification_question
        .clone()
        .unwrap_or_else(|| "请补充你希望我如何处理这个请求。".to_string());

    AssistantExecuteResponse {
        body: AssistantExecuteBody::Chat {
            payload: serde_json::json!({
                "content": question,
                "status": "completed",
                "pending_confirmation": false,
            }),
        },
        request_id: request_id.clone(),
        task_id: None,
        run_status: "completed".into(),
        evidence_refresh_notice: None,
        artifacts: Vec::new(),
        intent_detection: Some(intent_detection),
        task_plan: Some(task_plan),
        run_plan_summary: Some(
            AgentRunPlanSummary::for_intent(
                request_id,
                detected_intent,
                legacy_scene_hint,
                context_summary,
                "TaskPlan 要求先澄清，本轮不调用模型、工具或 Harness".to_string(),
            )
            .with_execution_state(
                "completed",
                "无需权限预检；本轮仅返回澄清问题".to_string(),
                Vec::new(),
                false,
            ),
        ),
        permission_preflight_summary: None,
    }
}

/// Route a unified assistant request through the harness task layer.
pub(crate) async fn route_assistant_execute(
    state: &Arc<AppState>,
    app_handle: &AppHandle,
    request: AssistantExecuteRequest,
) -> AppResult<AssistantExecuteResponse> {
    validate_assistant_domain_boundary(&request)?;
    let task_plan = build_or_validate_task_plan(&request)?;
    let agent_intent = agent_intent_for_task_plan(&task_plan);
    let legacy_intent = legacy_intent_for_task_plan(&task_plan);
    let task_policy = crate::ai_runtime::agent_task_policy::AgentTaskPolicy::from_input(
        crate::ai_runtime::agent_task_policy::AgentTaskPolicyInput::from_task_plan(
            &task_plan, &request,
        ),
    );
    let legacy_scene_hint = legacy_intent.scene();
    let mut intent_detection = request.detection_summary();
    intent_detection.detected_intent = agent_intent;
    intent_detection.reason = "Derived from validated TaskPlan.".into();
    intent_detection.source_hints = task_plan.source_hints.clone();
    let context_summary = request.context_summary();
    if task_plan.requires_clarification
        || matches!(task_plan.execution_mode, ExecutionMode::Clarification)
    {
        return Ok(clarification_response(
            task_plan,
            intent_detection,
            legacy_scene_hint,
            context_summary,
        ));
    }
    let skill_activation_plan = state
        .vault_path()
        .ok()
        .and_then(|vault| {
            let skills = crate::ai_runtime::skills::scan_all_metadata(&vault).ok()?;
            let index = crate::ai_runtime::skills::load_activation_index(&state.db).ok();
            Some(
                crate::ai_runtime::skills::build_skill_activation_plan_for_task_with_runtime(
                    &skills,
                    agent_intent,
                    &request.message,
                    &intent_detection.source_hints,
                    index.as_ref(),
                    Some(legacy_scene_hint),
                    Some(&state.db),
                ),
            )
        })
        .unwrap_or_else(|| crate::ai_types::SkillActivationPlanSummary {
            activated_skills: Vec::new(),
            requested_tools: Vec::new(),
            confirmation_required_tools: Vec::new(),
            blocked_capabilities: Vec::new(),
            skill_overlay_summary: "Skill activation skipped because no vault is available.".into(),
            degraded: false,
        });
    let _ = crate::ai_runtime::skills::record_skill_activation_matched(
        &state.db,
        &skill_activation_plan,
    );
    let route =
        crate::ai_runtime::agent_task_policy::resolve_for_task_policy(&state.db, &task_policy)?;
    let profile = crate::ai_runtime::prompt_profile::PromptProfile::load(&state.db)?;
    let persona_layers = crate::ai_runtime::persona_resolver::resolve_persona_for_agent(
        &profile,
        agent_intent,
        request.web_authorized,
        None,
    )
    .layer_summaries();
    let routing_override = crate::commands::ai_commands::AiSendRoutingOverride {
        resolved: route.resolved.clone(),
        slot: route.summary.slot,
        task_policy: task_policy.clone(),
        skill_activation_plan: Some(skill_activation_plan.clone()),
        failover_candidates: route.failover_candidates.clone(),
    };
    let task_result = crate::ai_runtime::harness_task::run_harness_task(
        state,
        app_handle,
        crate::ai_runtime::harness_task::HarnessTaskRequest::from_assistant_with_routing(
            request,
            task_plan.clone(),
            routing_override,
        ),
    )
    .await?;
    let mut response = crate::ai_runtime::harness_task::map_task_result_to_response(task_result);
    let _ =
        crate::ai_runtime::skills::record_skill_activation_used(&state.db, &skill_activation_plan);
    let permission_preflight =
        build_permission_preflight_summary(&skill_activation_plan, response.run_status.as_str());
    let permission_summary = permission_preflight.summary.clone();
    let blocked_reasons = blocked_reasons_for_response(&response);
    let degraded = response.run_status != "completed";
    response.intent_detection = Some(intent_detection);
    response.task_plan = Some(task_plan);
    response.run_plan_summary = Some(
        AgentRunPlanSummary::for_intent(
            response.request_id.clone(),
            agent_intent,
            legacy_scene_hint,
            context_summary,
            "复用当前 Harness 工具策略；本阶段不扩大工具权限".to_string(),
        )
        .with_execution_state(
            response.run_status.clone(),
            permission_summary.clone(),
            blocked_reasons,
            degraded,
        )
        .with_model_route(route.summary)
        .with_persona_layers(persona_layers)
        .with_skill_activation_plan(skill_activation_plan)
        .with_audit_summary(AgentAuditSummary {
            tool_events: response.artifacts.len() as u32,
            confirmed_tools: response
                .artifacts
                .iter()
                .filter(|artifact| is_pending_confirmation_artifact(artifact))
                .count() as u32,
            denied_tools: 0,
            sanitized: true,
        }),
    );
    response.permission_preflight_summary = Some(permission_preflight);
    Ok(response)
}

/// Unified assistant entry point for the React frontend.
#[tauri::command]
pub async fn assistant_execute(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    request: AssistantExecuteRequest,
) -> AppResult<AssistantExecuteResponse> {
    route_assistant_execute(state.inner(), &app_handle, request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_types::{
        CapabilitySlot, ExecutionMode, OutputMode, RetrievalMode, TaskPlanConfidence,
        TaskPlanIntent, WebMode,
    };

    fn request_with_note_content(
        note_path: Option<&str>,
        note_content: Option<&str>,
    ) -> AssistantExecuteRequest {
        AssistantExecuteRequest {
            ai_domain: AssistantAiDomain::Normal,
            agent_intent: None,
            intent: Some(AssistantIntent::Chat),
            intent_detection: None,
            task_plan: None,
            context_references: Vec::new(),
            runtime_documents: Vec::new(),
            message: "test".into(),
            note_path: note_path.map(str::to_string),
            note_content: note_content.map(str::to_string),
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

    #[test]
    fn note_content_requires_note_path_boundary() {
        let leaked_content = request_with_note_content(None, Some("secret note body"));
        assert!(validate_note_content_boundary(&leaked_content).is_err());

        let empty_content = request_with_note_content(None, Some("   \n\t"));
        assert!(validate_note_content_boundary(&empty_content).is_ok());

        let scoped_content = request_with_note_content(Some("notes/a.md"), Some("note body"));
        assert!(validate_note_content_boundary(&scoped_content).is_ok());
    }

    #[test]
    fn classified_domain_accepts_only_classified_path_without_note_content() {
        let mut request = request_with_note_content(Some(".classified/secret.md"), None);
        request.ai_domain = AssistantAiDomain::Classified;
        assert!(validate_assistant_domain_boundary(&request).is_ok());

        let mut normal_rejection = request_with_note_content(Some(".classified/secret.md"), None);
        normal_rejection.ai_domain = AssistantAiDomain::Normal;
        assert!(validate_assistant_domain_boundary(&normal_rejection).is_err());

        let mut leaked = request_with_note_content(Some(".classified/secret.md"), Some("secret"));
        leaked.ai_domain = AssistantAiDomain::Classified;
        assert!(validate_assistant_domain_boundary(&leaked).is_err());
    }

    #[test]
    fn clarification_response_stays_out_of_harness_shape() {
        let task_plan = TaskPlanSummary {
            intent: TaskPlanIntent::Chat,
            confidence: TaskPlanConfidence::Low,
            evidence_need: None,
            context_need: None,
            operation_kind: None,
            output_shape: None,
            context_references: Vec::new(),
            retrieval_mode: RetrievalMode::None,
            web_mode: WebMode::Disabled,
            model_slot: CapabilitySlot::Fast,
            execution_mode: ExecutionMode::Clarification,
            output_mode: OutputMode::Diagnostic,
            artifact_plan: Vec::new(),
            requires_clarification: true,
            clarification_question: Some("你希望我先查笔记还是直接回答？".into()),
            source_hints: vec!["test:clarification".into()],
            edit_target: None,
        };
        let intent_detection = IntentDetectionSummary {
            detected_intent: AgentIntent::Chat,
            confidence: 0.35,
            reason: "test".into(),
            alternatives: Vec::new(),
            fallback_behavior: "ask".into(),
            source_hints: task_plan.source_hints.clone(),
        };

        let response = clarification_response(
            task_plan,
            intent_detection,
            crate::ai_runtime::AiScene::KnowledgeLookup,
            vec!["无额外上下文".into()],
        );

        assert!(response.task_id.is_none());
        assert!(response.artifacts.is_empty());
        assert!(response.task_plan.is_some());
        match response.body {
            AssistantExecuteBody::Chat { payload } => {
                assert_eq!(
                    payload.get("content").and_then(serde_json::Value::as_str),
                    Some("你希望我先查笔记还是直接回答？")
                );
            }
            _ => panic!("clarification response must be a chat body"),
        }
    }
}
