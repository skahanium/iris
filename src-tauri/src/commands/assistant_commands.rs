//! Unified assistant IPC facade — routes intents to existing workflows.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::ai_runtime::assistant_facade::{
    agent_intent_from_legacy, legacy_intent_for_agent, AssistantIntent,
};
use crate::ai_runtime::chapter_workflow::ChapterInfo;
use crate::ai_runtime::document_workflow::DocumentCheckResult;
use crate::ai_runtime::harness::UsageSource;
use crate::ai_runtime::harness_support::{
    save_harness_checkpoint, HarnessCheckpoint, HarnessCheckpointMeta,
};
use crate::ai_runtime::model_gateway::{LlmMessage, MessageRole, TokenUsage, ToolCall};
use crate::ai_runtime::retrieval_scope::ContextScopeDto;
use crate::ai_runtime::task_plan::{
    agent_intent_for_task_plan, build_or_validate_task_plan, legacy_intent_for_task_plan,
};
use crate::ai_runtime::writing_workflow::WritingTaskOutput;
use crate::ai_runtime::{
    AgentAuditSummary, AgentIntent, AgentRunPlanSummary, CitationCheckResult, ContextReferenceWire,
    ExecutionMode, IntentDetectionSummary, OrganizeTaskResult, PermissionPreflightSummary,
    SkillActivationPlanSummary, SkillCapabilitySupportStatus, TaskPlanSummary,
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

fn detect_skillhub_direct_install(message: &str) -> Option<String> {
    let lower = message.to_lowercase();
    let has_skillhub_source = lower.contains("skillhub")
        || lower.contains("skillhub.cn/install/skillhub.md")
        || lower.contains("skillhub 商店")
        || lower.contains("skillhub商店");
    if !has_skillhub_source {
        return None;
    }

    for marker in ["安装", "install"] {
        let Some(idx) = lower.rfind(marker) else {
            continue;
        };
        let rest = &message[idx + marker.len()..];
        let candidate: String = rest
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if candidate.len() >= 2 && candidate.to_lowercase() != "skillhub" {
            return Some(candidate.to_lowercase());
        }
    }
    None
}

fn skill_installed(plan: &[crate::ai_runtime::skills::SkillListEntry], name: &str) -> bool {
    plan.iter()
        .any(|entry| entry.skill.name.eq_ignore_ascii_case(name))
}

async fn maybe_handle_skillhub_direct_install(
    state: &AppState,
    app_handle: &AppHandle,
    request: &AssistantExecuteRequest,
    agent_intent: AgentIntent,
    task_plan: &TaskPlanSummary,
    skill_activation_plan: &SkillActivationPlanSummary,
    intent_detection: &IntentDetectionSummary,
) -> AppResult<Option<AssistantExecuteResponse>> {
    if !matches!(agent_intent, AgentIntent::SkillManagement) {
        return Ok(None);
    }
    let source_hint_skill = intent_detection.source_hints.iter().find_map(|hint| {
        hint.strip_prefix("skillhub:direct_install:")
            .map(str::to_string)
    });
    let Some(skill_name) =
        source_hint_skill.or_else(|| detect_skillhub_direct_install(&request.message))
    else {
        return Ok(None);
    };

    let request_id = uuid::Uuid::new_v4().to_string();
    let task_policy = crate::ai_runtime::agent_task_policy::AgentTaskPolicy::from_input(
        crate::ai_runtime::agent_task_policy::AgentTaskPolicyInput::from_task_plan(
            task_plan, request,
        ),
    );
    let legacy_scene_hint = task_policy.legacy_scene();
    let session_id = crate::ai_runtime::session::SessionManager::ensure(
        &state.db,
        legacy_scene_hint,
        request.note_path.as_deref(),
    )?;
    let task_id = crate::ai_runtime::agent_task::AgentTaskRuntime::create_task(
        &state.db,
        crate::ai_runtime::agent_task::CreateTaskInput {
            request_id: request_id.clone(),
            session_id,
            kind: crate::ai_runtime::agent_task::AgentTaskKind::Complex,
            user_input: request.message.clone(),
            budget_policy: serde_json::json!({
                "mode": "assistant_workflow",
                "intent": "skill_management",
                "source": "skillhub",
            }),
        },
    )?;
    let vault = match state.vault_path() {
        Ok(vault) => vault,
        Err(err) => {
            let _ = crate::ai_runtime::agent_task::AgentTaskRuntime::fail_safe(
                &state.db,
                &task_id,
                "VAULT_SCOPE_ERROR",
            );
            return Err(err);
        }
    };
    let installed = crate::ai_runtime::skill_install_service::list_skills(&state.db, &vault, None)?;
    if skill_installed(&installed, &skill_name) {
        crate::ai_runtime::agent_task::AgentTaskRuntime::record_step(
            &state.db,
            &task_id,
            "skill_management",
            crate::ai_runtime::agent_task::AgentTaskStatus::Completed,
            "skillhub install request summarized in agent_tasks",
            "requested skill was already installed",
            serde_json::json!({
                "summary": "requested skill already installed",
                "skill_name": skill_name.clone(),
            }),
        )?;
        crate::ai_runtime::agent_task::AgentTaskRuntime::complete_task(&state.db, &task_id)?;
        return Ok(Some(AssistantExecuteResponse {
            body: AssistantExecuteBody::Chat {
                payload: serde_json::json!({
                   "content": format!("Skill `{skill_name}` 已安装。"),
                   "status": "completed",
                   "pending_confirmation": false,
                }),
            },
            request_id,
            task_id: Some(task_id),
            run_status: "completed".into(),
            evidence_refresh_notice: None,
            artifacts: Vec::new(),
            intent_detection: Some(intent_detection.clone()),
            task_plan: Some(task_plan.clone()),
            run_plan_summary: None,
            permission_preflight_summary: None,
        }));
    }

    let args = serde_json::json!({
        "source": "registry",
        "registry": "skillhub",
        "path_or_url": skill_name,
        "scope": "vault",
        "reason": "用户在 AI 对话中明确要求从 SkillHub 安装该技能"
    });
    let arguments = serde_json::to_string(&args).unwrap_or_default();
    let tool_call = ToolCall::new(
        format!("direct-skillhub-{}", uuid::Uuid::new_v4()),
        "skills_install",
        arguments.clone(),
    );

    crate::llm::safe_lock(&state.ai.pending_tool_calls).insert(
        tool_call.id.clone(),
        crate::app::PendingToolCall {
            tool_name: "skills_install".into(),
            arguments: arguments.clone(),
            request_id: request_id.clone(),
            scene: legacy_scene_hint,
            note_path: request.note_path.clone(),
            file_id: None,
            web_search_enabled: request.web_authorized,
            autonomy_level: task_policy.autonomy_level,
            skill_allowed_tools: skill_activation_plan.allowed_tools(),
            skill_activation_plan: Some(skill_activation_plan.clone()),
        },
    );

    let checkpoint = HarnessCheckpoint {
        meta: HarnessCheckpointMeta {
            scene: legacy_scene_hint.profile().into(),
            session_id,
            note_path: request.note_path.clone(),
            note_title: None,
            selection_excerpt: request.selection.clone(),
            cold_start_packets: Vec::new(),
            web_search_enabled: request.web_authorized,
            depth: 0,
            capability_slot: None,
            provider_id: None,
            model: None,
            endpoint_family: None,
            thinking: None,
            output_budget: None,
            skill_activation_plan: Some(skill_activation_plan.clone()),
            task_policy: Some(task_policy),
        },
        round: 1,
        messages: vec![
            LlmMessage {
                role: MessageRole::User,
                content: request.message.clone().into(),
                ..Default::default()
            },
            LlmMessage {
                role: MessageRole::Assistant,
                content: format!(
                    "将通过 SkillHub registry 安装 `{}`，等待用户确认。",
                    skill_name
                )
                .into(),
                tool_calls: Some(vec![tool_call.clone()]),
                ..Default::default()
            },
        ],
        tool_calls: vec![tool_call.clone()],
        tool_results: vec![serde_json::json!({
            "tool_call_id": tool_call.id,
            "status": "pending_confirmation",
        })],
        evidence_packets: Vec::new(),
        usage: TokenUsage::default(),
        usage_source: UsageSource::Estimated,
        bonus_round_used: false,
    };
    crate::ai_runtime::trace::TraceRecorder::start(&state.db, &request_id, legacy_scene_hint)?;
    crate::ai_runtime::trace::TraceRecorder::update_status(
        &state.db,
        &request_id,
        crate::ai_runtime::trace::TraceStatus::AwaitingToolConfirmation,
    )?;
    save_harness_checkpoint(&state.db, &request_id, &checkpoint)?;
    crate::ai_runtime::agent_task::AgentTaskRuntime::record_step(
        &state.db,
        &task_id,
        "skill_management",
        crate::ai_runtime::agent_task::AgentTaskStatus::AwaitingConfirmation,
        "skillhub install request summarized in agent_tasks",
        "waiting for tool confirmation",
        serde_json::json!({
            "summary": "waiting for skill install confirmation",
            "skill_name": skill_name.clone(),
            "tool_name": "skills_install",
            "next_action": "wait_for_user_confirmation"
        }),
    )?;
    crate::ai_runtime::agent_task::AgentTaskRuntime::await_confirmation(&state.db, &task_id)?;

    let mut confirm_request = serde_json::json!({
        "request_id": request_id,
        "tool_call_id": tool_call.id,
        "tool_name": "skills_install",
        "arguments": args,
        "permissionEffects": [],
    });
    let req = crate::ai_runtime::skill_install_service::SkillInstallRequest {
        source: crate::ai_runtime::skill_registry::SkillInstallSource::Registry,
        path_or_url: skill_name.clone(),
        scope: crate::ai_runtime::skills::SkillScope::Vault,
        subpath: None,
        registry: Some("skillhub".into()),
        expected_sha256: None,
    };
    if let Ok(preview) =
        crate::ai_runtime::skill_install_service::preview_install(&vault, &req).await
    {
        confirm_request["preview"] = preview;
    }
    let _ = app_handle.emit("ai:tool_confirm_request", &confirm_request);

    Ok(Some(AssistantExecuteResponse {
        body: AssistantExecuteBody::Chat {
            payload: serde_json::json!({
                "content": format!("准备从 SkillHub 安装 `{skill_name}`，等待确认。"),
                "status": "pending_tools",
                "pending_confirmation": true,
            }),
        },
        request_id: request_id.clone(),
        task_id: Some(task_id),
        run_status: "pending_confirmation".into(),
        evidence_refresh_notice: None,
        artifacts: vec![crate::ai_runtime::harness_task::HarnessArtifactWire {
            kind: "task_process".into(),
            title: "工具确认".into(),
            status: "pending".into(),
            source_task: "skill_management".into(),
            evidence_count: 0,
            payload: serde_json::json!({
                "schema": "task_process",
                "status": "pending_confirmation",
                "request_id": request_id,
                "tool_call_id": tool_call.id,
                "tool_name": "skills_install",
                "next_action": "wait_for_user_confirmation",
            }),
        }],
        intent_detection: Some(intent_detection.clone()),
        task_plan: Some(task_plan.clone()),
        run_plan_summary: None,
        permission_preflight_summary: Some(build_permission_preflight_summary(
            skill_activation_plan,
            "pending_confirmation",
        )),
    }))
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
            requested_capabilities: Vec::new(),
            confirmation_required_tools: Vec::new(),
            blocked_capabilities: Vec::new(),
            skill_overlay_summary: "Skill activation skipped because no vault is available.".into(),
            degraded: false,
        });
    let _ = crate::ai_runtime::skill_install_service::record_skill_activation_matched(
        &state.db,
        &skill_activation_plan,
    );
    if let Some(response) = maybe_handle_skillhub_direct_install(
        state,
        app_handle,
        &request,
        agent_intent,
        &task_plan,
        &skill_activation_plan,
        &intent_detection,
    )
    .await?
    {
        return Ok(response);
    }
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
    let _ = crate::ai_runtime::skill_install_service::record_skill_activation_used(
        &state.db,
        &skill_activation_plan,
    );
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
    fn detects_skillhub_direct_install_target() {
        assert_eq!(
            detect_skillhub_direct_install(
                "请根据 https://skillhub.cn/install/skillhub.md 安装self-improving技能"
            )
            .as_deref(),
            Some("self-improving")
        );
    }

    #[test]
    fn clarification_response_stays_out_of_harness_shape() {
        let task_plan = TaskPlanSummary {
            intent: TaskPlanIntent::Chat,
            confidence: TaskPlanConfidence::Low,
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
