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
use crate::ai_runtime::writing_workflow::WritingTaskOutput;
use crate::ai_runtime::{
    AgentAuditSummary, AgentIntent, AgentRunPlanSummary, CitationCheckResult,
    IntentDetectionSummary, OrganizeTaskResult, PermissionPreflightSummary,
    SkillActivationPlanSummary, SkillCapabilitySupportStatus,
};
use crate::app::AppState;
use crate::error::AppResult;

/// Unified assistant execution request (camelCase for TypeScript IPC).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantExecuteRequest {
    #[serde(default)]
    pub agent_intent: Option<AgentIntent>,
    #[serde(default)]
    pub intent: Option<AssistantIntent>,
    #[serde(default)]
    pub intent_detection: Option<IntentDetectionSummary>,
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
        if self.web_authorized {
            summary.push("允许联网检索".to_string());
        }
        if summary.is_empty() {
            summary.push("无额外上下文".to_string());
        }
        summary
    }

    fn estimated_context_tokens(&self) -> usize {
        let chars = self
            .note_content
            .as_deref()
            .unwrap_or_default()
            .chars()
            .count()
            + self
                .selection
                .as_deref()
                .unwrap_or_default()
                .chars()
                .count()
            + self
                .cursor_context
                .as_deref()
                .unwrap_or_default()
                .chars()
                .count()
            + self.message.chars().count();
        chars / 3
    }
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
            .filter(|artifact| artifact.kind == "tool_confirmation")
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
    pub run_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_refresh_notice: Option<String>,
    pub artifacts: Vec<crate::ai_runtime::harness_task::HarnessArtifactWire>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent_detection: Option<IntentDetectionSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_plan_summary: Option<AgentRunPlanSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_preflight_summary: Option<PermissionPreflightSummary>,
}

/// Route a unified assistant request through the harness task layer.
pub(crate) async fn route_assistant_execute(
    state: &AppState,
    app_handle: &AppHandle,
    request: AssistantExecuteRequest,
) -> AppResult<AssistantExecuteResponse> {
    crate::commands::ai_commands::validate_ai_note_path(request.note_path.as_deref())?;
    let agent_intent = request.effective_agent_intent();
    let legacy_scene = agent_intent.scene();
    let intent_detection = request.detection_summary();
    let context_summary = request.context_summary();
    let skill_activation_plan = state
        .vault_path()
        .ok()
        .and_then(|vault| {
            let skills = crate::ai_runtime::skills::scan_all_metadata(&vault).ok()?;
            let index = crate::ai_runtime::skills::load_activation_index(&state.db).ok();
            Some(crate::ai_runtime::skills::build_skill_activation_plan(
                &skills,
                legacy_scene,
                agent_intent,
                &request.message,
                &intent_detection.source_hints,
                index.as_ref(),
            ))
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
    let route = crate::llm::config::resolve_capability_route(
        &state.db,
        crate::llm::config::CapabilityRouteInput {
            intent: agent_intent,
            context_tokens: request.estimated_context_tokens(),
            has_images: matches!(agent_intent, AgentIntent::VisionChat),
            needs_tools: matches!(
                agent_intent,
                AgentIntent::Research | AgentIntent::SkillManagement
            ),
            needs_reasoning: matches!(
                agent_intent,
                AgentIntent::Research | AgentIntent::CitationCheck
            ),
            privacy_preference: crate::llm::config::PrivacyPreference::ExternalAllowed,
        },
    )?;
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
        skill_activation_plan: Some(skill_activation_plan.clone()),
    };
    let task_result = crate::ai_runtime::harness_task::run_harness_task(
        state,
        app_handle,
        crate::ai_runtime::harness_task::HarnessTaskRequest::from_assistant_with_routing(
            request,
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
    response.run_plan_summary = Some(
        AgentRunPlanSummary::for_intent(
            response.request_id.clone(),
            agent_intent,
            legacy_scene,
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
                .filter(|artifact| artifact.kind == "tool_confirmation")
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
    route_assistant_execute(state.inner().as_ref(), &app_handle, request).await
}
