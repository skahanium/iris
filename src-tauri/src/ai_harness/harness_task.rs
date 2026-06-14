//! Unified harness task request/result/artifact contract.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::ai_runtime::assistant_facade::{
    parse_document_check_type, parse_organize_task_type, AssistantIntent,
};
use crate::ai_runtime::chapter_workflow::{self, ChapterInfo, ChapterWritingInput};
use crate::ai_runtime::document_workflow::DocumentCheckInput;
use crate::ai_runtime::document_workflow::DocumentCheckResult;
use crate::ai_runtime::model_gateway::TokenUsage;
use crate::ai_runtime::writing_workflow::WritingTaskOutput;
use crate::ai_runtime::{
    CitationCheckInput, CitationCheckResult, CitationCheckScope, OrganizeTaskInput,
    OrganizeTaskResult, OrganizeTaskScope, PatchProposal,
};
use crate::app::AppState;
use crate::commands::assistant_commands::AssistantExecuteRequest;
use crate::commands::writing_commands::WritingTaskInputIpc;
use crate::commands::{
    ai_commands, citation_commands, document_commands, organize_commands, research_commands,
    writing_commands,
};
use crate::error::{AppError, AppResult};

/// Unified run status for all assistant intents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessRunStatus {
    Completed,
    PendingConfirmation,
    Failed,
    Aborted,
}

/// Artifact kinds returned by harness task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HarnessArtifact {
    Message {
        content: String,
        citation_valid: bool,
    },
    Patches {
        patches: Vec<PatchProposal>,
    },
    CitationReport {
        report: CitationCheckResult,
    },
    OrganizeReport {
        report: OrganizeTaskResult,
    },
    ResearchReport {
        payload: serde_json::Value,
    },
    DocumentCheck {
        report: DocumentCheckResult,
    },
    ChapterWriting {
        payload: serde_json::Value,
    },
    ToolConfirmation {
        request_id: String,
        tool_call_id: String,
    },
    LegacyPayload {
        intent: String,
        json: serde_json::Value,
    },
}

/// Wire shape for optional frontend artifact panel (migration).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessArtifactWire {
    pub kind: String,
    pub title: String,
    pub status: String,
    pub source_task: String,
    pub evidence_count: u32,
    pub payload: serde_json::Value,
}

/// Unified harness task request (maps from `AssistantExecuteRequest`).
#[derive(Debug, Clone)]
pub(crate) struct HarnessTaskRequest {
    pub(crate) assistant: AssistantExecuteRequest,
    pub(crate) routing_override: Option<ai_commands::AiSendRoutingOverride>,
}

impl HarnessTaskRequest {
    pub(crate) fn from_assistant_with_routing(
        assistant: AssistantExecuteRequest,
        routing_override: ai_commands::AiSendRoutingOverride,
    ) -> Self {
        Self {
            assistant,
            routing_override: Some(routing_override),
        }
    }
}

/// Internal task result before mapping to IPC.
#[derive(Debug, Clone, Serialize)]
pub struct HarnessTaskResult {
    pub request_id: String,
    pub run_status: HarnessRunStatus,
    pub artifacts: Vec<HarnessArtifact>,
    pub artifact_wires: Vec<HarnessArtifactWire>,
    pub evidence_packet_ids: Vec<String>,
    pub usage: Option<TokenUsage>,
    pub evidence_refresh_notice: Option<String>,
    pub chat_payload: Option<serde_json::Value>,
    pub writing: Option<WritingTaskOutput>,
    pub citation: Option<CitationCheckResult>,
    pub organize: Option<OrganizeTaskResult>,
    pub research: Option<serde_json::Value>,
    pub chapter: Option<serde_json::Value>,
    pub document: Option<DocumentCheckResult>,
}

/// Execute unified assistant task and collect artifacts.
fn run_status_wire(status: HarnessRunStatus) -> &'static str {
    match status {
        HarnessRunStatus::Completed => "completed",
        HarnessRunStatus::PendingConfirmation => "pending_confirmation",
        HarnessRunStatus::Failed => "failed",
        HarnessRunStatus::Aborted => "aborted",
    }
}

fn emit_workflow_trace(app_handle: &AppHandle, request_id: &str, task_name: &str, status: &str) {
    use crate::ai_runtime::harness::{HarnessPhase, HarnessTraceEvent};
    let _ = app_handle.emit(
        "ai:harness_trace",
        &HarnessTraceEvent {
            request_id: request_id.to_string(),
            round: 0,
            phase: HarnessPhase::ToolStart,
            tool_name: task_name.to_string(),
            status: status.to_string(),
            message: None,
            output_preview: None,
        },
    );
}

fn skill_scope_wire(scope: crate::ai_runtime::skills::SkillScope) -> &'static str {
    match scope {
        crate::ai_runtime::skills::SkillScope::Global => "Global",
        crate::ai_runtime::skills::SkillScope::Vault => "Vault",
    }
}

fn legacy_skill_overlay_from_plan(
    state: &AppState,
    scene: crate::ai_runtime::AiScene,
    user_message: &str,
    plan: Option<&crate::ai_types::SkillActivationPlanSummary>,
) -> String {
    let Some(plan) = plan.filter(|plan| !plan.activated_skills.is_empty()) else {
        return String::new();
    };
    let Ok(vault) = state.vault_path() else {
        return String::new();
    };
    let Ok(skills) = crate::ai_runtime::skills::scan_all(&vault) else {
        return String::new();
    };
    let selected: Vec<_> = skills
        .into_iter()
        .filter(|skill| {
            plan.activated_skills.iter().any(|active| {
                active.blocked_capabilities.is_empty()
                    && active.name == skill.name
                    && active.scope == skill_scope_wire(skill.scope)
            })
        })
        .collect();
    if selected.is_empty() {
        return String::new();
    }
    crate::ai_runtime::skills::inject_into_prompt(&selected, scene, user_message)
}

fn apply_skill_overlay_to_goal(goal: &str, overlay: &str) -> String {
    let overlay = overlay.trim();
    if overlay.is_empty() {
        goal.to_string()
    } else {
        format!("{goal}\n\n## Active Skill Guidance\n{overlay}")
    }
}

/// Execute unified assistant task and collect artifacts.
pub(crate) async fn run_harness_task(
    state: &AppState,
    app_handle: &AppHandle,
    task: HarnessTaskRequest,
) -> AppResult<HarnessTaskResult> {
    let request = task.assistant;
    let routing_override = task.routing_override;
    crate::commands::ai_commands::validate_ai_note_path(request.note_path.as_deref())?;
    let legacy_intent = request.effective_legacy_intent();
    let skill_activation_plan = routing_override
        .as_ref()
        .and_then(|route| route.skill_activation_plan.as_ref());
    let skill_overlay = legacy_skill_overlay_from_plan(
        state,
        legacy_intent.scene(),
        &request.message,
        skill_activation_plan,
    );
    match legacy_intent {
        AssistantIntent::Writing => {
            let note_path = request
                .note_path
                .clone()
                .ok_or_else(|| AppError::msg("写作任务需要 notePath"))?;
            let selection = request
                .selection
                .as_ref()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| AppError::msg("写作任务需要选区"))?
                .clone();
            let cursor_context = request
                .cursor_context
                .clone()
                .or(request.note_content.clone())
                .unwrap_or_default();
            let input = WritingTaskInputIpc {
                target_path: note_path,
                base_content_hash: resolve_content_hash(
                    request.note_content.as_deref(),
                    request.base_content_hash.as_deref(),
                ),
                selection: Some(selection),
                cursor_context,
                writing_goal: apply_skill_overlay_to_goal(&request.message, &skill_overlay),
                web_authorized: request.web_authorized,
            };
            let trace_id = uuid::Uuid::new_v4().to_string();
            emit_workflow_trace(app_handle, &trace_id, "writing", "running");
            let payload = writing_commands::execute_writing_task(state, app_handle, input).await?;
            emit_workflow_trace(app_handle, &payload.request_id, "writing", "ok");
            Ok(task_result_from_writing(payload))
        }
        AssistantIntent::Citation => {
            let trace_id = uuid::Uuid::new_v4().to_string();
            emit_workflow_trace(app_handle, &trace_id, "citation", "running");
            let note_path = request
                .note_path
                .clone()
                .ok_or_else(|| AppError::msg("引用检查需要 notePath"))?;
            let paragraph_text = request
                .paragraph_text
                .clone()
                .or(request.selection.clone())
                .filter(|t| !t.is_empty())
                .ok_or_else(|| AppError::msg("引用检查需要段落或选区文本"))?;
            let input = CitationCheckInput {
                paragraph_text,
                document_path: note_path,
                scope: citation_scope_from_dto(request.context_scope.clone()),
                web_authorized: request.web_authorized,
            };
            let payload =
                citation_commands::execute_citation_check(state, app_handle, input).await?;
            emit_workflow_trace(app_handle, &payload.request_id, "citation", "ok");
            Ok(task_result_from_citation(payload))
        }
        AssistantIntent::Organize => {
            let trace_id = uuid::Uuid::new_v4().to_string();
            emit_workflow_trace(app_handle, &trace_id, "organize", "running");
            let input = OrganizeTaskInput {
                scope: organize_scope_from_dto(request.context_scope.clone()),
                task_type: parse_organize_task_type(request.organize_task_type.as_deref()),
            };
            let payload =
                organize_commands::execute_organize_task(state, app_handle, input).await?;
            emit_workflow_trace(app_handle, &payload.request_id, "organize", "ok");
            Ok(task_result_from_organize(payload))
        }
        AssistantIntent::Research => {
            let trace_id = uuid::Uuid::new_v4().to_string();
            emit_workflow_trace(app_handle, &trace_id, "research", "running");
            let payload = research_commands::execute_research_task(
                state,
                app_handle,
                apply_skill_overlay_to_goal(&request.message, &skill_overlay),
                Some(request.web_authorized),
            )
            .await?;
            let rid = payload
                .get("request_id")
                .and_then(|v| v.as_str())
                .unwrap_or(&trace_id);
            emit_workflow_trace(app_handle, rid, "research", "ok");
            Ok(task_result_from_research(payload))
        }
        AssistantIntent::Chapter => {
            let trace_id = uuid::Uuid::new_v4().to_string();
            emit_workflow_trace(app_handle, &trace_id, "chapter", "running");
            let note_path = request
                .note_path
                .clone()
                .ok_or_else(|| AppError::msg("章节写作需要 notePath"))?;
            let chapter =
                resolve_chapter(request.chapter.clone(), request.note_content.as_deref())?;
            let input = ChapterWritingInput {
                target_path: note_path,
                base_content_hash: resolve_content_hash(
                    request.note_content.as_deref(),
                    request.base_content_hash.as_deref(),
                ),
                chapter,
                writing_goal: apply_skill_overlay_to_goal(&request.message, &skill_overlay),
                web_authorized: request.web_authorized,
            };
            let payload =
                document_commands::execute_chapter_writing(state, app_handle, input).await?;
            emit_workflow_trace(app_handle, &payload.request_id, "chapter", "ok");
            Ok(task_result_from_chapter(payload))
        }
        AssistantIntent::Document => {
            let trace_id = uuid::Uuid::new_v4().to_string();
            emit_workflow_trace(app_handle, &trace_id, "document", "running");
            let note_path = request
                .note_path
                .clone()
                .ok_or_else(|| AppError::msg("文档检查需要 notePath"))?;
            let note_content = request
                .note_content
                .clone()
                .ok_or_else(|| AppError::msg("文档检查需要 noteContent"))?;
            let input = DocumentCheckInput {
                target_path: note_path,
                content: note_content.clone(),
                check_type: parse_document_check_type(request.document_check_type.as_deref()),
                web_authorized: request.web_authorized,
                base_content_hash: resolve_content_hash(
                    Some(note_content.as_str()),
                    request.base_content_hash.as_deref(),
                ),
            };
            let payload =
                document_commands::execute_document_check(state, app_handle, input).await?;
            emit_workflow_trace(app_handle, &payload.request_id, "document", "ok");
            Ok(task_result_from_document(payload))
        }
        AssistantIntent::Chat | AssistantIntent::Knowledge => {
            let trace_id = uuid::Uuid::new_v4().to_string();
            emit_workflow_trace(app_handle, &trace_id, "chat", "running");
            let scene = legacy_intent.scene().profile().to_string();
            let payload = ai_commands::execute_ai_send_message_with_routing(
                state,
                app_handle,
                scene,
                request.session_id,
                request.message.clone(),
                request.images.clone(),
                request.selected_packet_ids.clone(),
                request.note_path.clone(),
                request.context_scope.clone(),
                Some(request.web_authorized),
                Some(request.new_session),
                routing_override,
            )
            .await?;
            let rid = payload
                .get("request_id")
                .and_then(|v| v.as_str())
                .unwrap_or(&trace_id);
            let status = if payload["pending_confirmation"].as_bool() == Some(true)
                || payload["status"].as_str() == Some("pending_tools")
            {
                "pending"
            } else {
                "ok"
            };
            emit_workflow_trace(app_handle, rid, "chat", status);
            Ok(task_result_from_chat(payload))
        }
    }
}

fn resolve_content_hash(note_content: Option<&str>, provided: Option<&str>) -> String {
    if let Some(hash) = provided.filter(|h| !h.is_empty()) {
        return hash.to_string();
    }
    note_content
        .map(crate::ai_runtime::writing_workflow::compute_content_hash)
        .unwrap_or_default()
}

fn citation_scope_from_dto(
    dto: Option<crate::ai_runtime::retrieval_scope::ContextScopeDto>,
) -> Option<CitationCheckScope> {
    dto.map(|scope| CitationCheckScope {
        paths: scope.paths,
        path_prefixes: scope.path_prefixes,
        corpus_ids: if scope.corpus_ids.is_empty() {
            None
        } else {
            Some(scope.corpus_ids)
        },
    })
}

fn organize_scope_from_dto(
    dto: Option<crate::ai_runtime::retrieval_scope::ContextScopeDto>,
) -> Option<OrganizeTaskScope> {
    dto.map(|scope| OrganizeTaskScope {
        paths: scope.paths,
        path_prefixes: scope.path_prefixes,
        corpus_ids: if scope.corpus_ids.is_empty() {
            None
        } else {
            Some(scope.corpus_ids)
        },
    })
}

fn resolve_chapter(
    chapter: Option<ChapterInfo>,
    note_content: Option<&str>,
) -> AppResult<ChapterInfo> {
    if let Some(ch) = chapter {
        return Ok(ch);
    }
    let content = note_content.ok_or_else(|| AppError::msg("章节任务需要笔记正文"))?;
    chapter_workflow::parse_chapters(content)
        .into_iter()
        .next()
        .ok_or_else(|| AppError::msg("当前文档没有可识别的章节结构"))
}

fn task_result_from_chat(payload: serde_json::Value) -> HarnessTaskResult {
    let request_id = payload["request_id"].as_str().unwrap_or("").to_string();
    let pending = payload["pending_confirmation"].as_bool().unwrap_or(false)
        || payload["status"].as_str() == Some("pending_tools");
    let run_status = if pending {
        HarnessRunStatus::PendingConfirmation
    } else {
        HarnessRunStatus::Completed
    };
    let content = payload["content"].as_str().unwrap_or("").to_string();
    let citation_valid = payload["citation_valid"].as_bool().unwrap_or(true);
    let packet_ids: Vec<String> = payload["evidence_packets"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.get("id").and_then(|id| id.as_str()))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    let usage: Option<TokenUsage> = serde_json::from_value(payload["usage"].clone()).ok();
    let mut artifacts = vec![HarnessArtifact::Message {
        content: content.clone(),
        citation_valid,
    }];
    if pending {
        if let Some(tc) = payload["tool_calls"].as_array().and_then(|a| a.first()) {
            if let (Some(rid), Some(tid)) = (
                payload["request_id"].as_str(),
                tc.get("id").and_then(|v| v.as_str()),
            ) {
                artifacts.push(HarnessArtifact::ToolConfirmation {
                    request_id: rid.to_string(),
                    tool_call_id: tid.to_string(),
                });
            }
        }
    }
    let artifact_wires = artifacts_to_wires(&artifacts, "chat");
    HarnessTaskResult {
        request_id,
        run_status,
        artifacts,
        artifact_wires,
        evidence_packet_ids: packet_ids,
        usage,
        evidence_refresh_notice: None,
        chat_payload: Some(payload),
        writing: None,
        citation: None,
        organize: None,
        research: None,
        chapter: None,
        document: None,
    }
}

fn task_result_from_writing(payload: WritingTaskOutput) -> HarnessTaskResult {
    let artifacts = vec![
        HarnessArtifact::Patches {
            patches: payload.patches.clone(),
        },
        HarnessArtifact::LegacyPayload {
            intent: "writing".into(),
            json: serde_json::to_value(&payload).unwrap_or_default(),
        },
    ];
    let artifact_wires = artifacts_to_wires(&artifacts, "writing");
    HarnessTaskResult {
        request_id: payload.request_id.clone(),
        run_status: HarnessRunStatus::Completed,
        artifacts,
        artifact_wires,
        evidence_packet_ids: payload.evidence_used.iter().map(|p| p.id.clone()).collect(),
        usage: Some(payload.total_tokens.clone()),
        evidence_refresh_notice: None,
        chat_payload: None,
        writing: Some(payload),
        citation: None,
        organize: None,
        research: None,
        chapter: None,
        document: None,
    }
}

fn task_result_from_citation(payload: CitationCheckResult) -> HarnessTaskResult {
    let artifacts = vec![HarnessArtifact::CitationReport {
        report: payload.clone(),
    }];
    let artifact_wires = artifacts_to_wires(&artifacts, "citation");
    HarnessTaskResult {
        request_id: payload.request_id.clone(),
        run_status: HarnessRunStatus::Completed,
        artifacts,
        artifact_wires,
        evidence_packet_ids: vec![],
        usage: None,
        evidence_refresh_notice: None,
        chat_payload: None,
        writing: None,
        citation: Some(payload),
        organize: None,
        research: None,
        chapter: None,
        document: None,
    }
}

fn task_result_from_organize(payload: OrganizeTaskResult) -> HarnessTaskResult {
    let artifacts = vec![HarnessArtifact::OrganizeReport {
        report: payload.clone(),
    }];
    let artifact_wires = artifacts_to_wires(&artifacts, "organize");
    HarnessTaskResult {
        request_id: "organize".into(),
        run_status: HarnessRunStatus::Completed,
        artifacts,
        artifact_wires,
        evidence_packet_ids: vec![],
        usage: None,
        evidence_refresh_notice: None,
        chat_payload: None,
        writing: None,
        citation: None,
        organize: Some(payload),
        research: None,
        chapter: None,
        document: None,
    }
}

fn task_result_from_research(payload: serde_json::Value) -> HarnessTaskResult {
    let request_id = payload
        .get("request_id")
        .and_then(|v| v.as_str())
        .unwrap_or("research")
        .to_string();
    let artifacts = vec![
        HarnessArtifact::ResearchReport {
            payload: payload.clone(),
        },
        HarnessArtifact::LegacyPayload {
            intent: "research".into(),
            json: payload.clone(),
        },
    ];
    let artifact_wires = artifacts_to_wires(&artifacts, "research");
    HarnessTaskResult {
        request_id,
        run_status: HarnessRunStatus::Completed,
        artifacts,
        artifact_wires,
        evidence_packet_ids: vec![],
        usage: None,
        evidence_refresh_notice: None,
        chat_payload: None,
        writing: None,
        citation: None,
        organize: None,
        research: Some(payload),
        chapter: None,
        document: None,
    }
}

fn task_result_from_chapter(
    payload: crate::ai_runtime::chapter_workflow::ChapterWritingResult,
) -> HarnessTaskResult {
    let artifacts = vec![
        HarnessArtifact::ChapterWriting {
            payload: serde_json::to_value(&payload).unwrap_or_default(),
        },
        HarnessArtifact::Patches {
            patches: payload.patches.clone(),
        },
    ];
    let artifact_wires = artifacts_to_wires(&artifacts, "chapter");
    HarnessTaskResult {
        request_id: payload.request_id.clone(),
        run_status: HarnessRunStatus::Completed,
        artifacts,
        artifact_wires,
        evidence_packet_ids: payload.evidence_used.iter().map(|p| p.id.clone()).collect(),
        usage: Some(payload.total_tokens.clone()),
        evidence_refresh_notice: None,
        chat_payload: None,
        writing: None,
        citation: None,
        organize: None,
        research: None,
        chapter: Some(serde_json::to_value(&payload).unwrap_or_default()),
        document: None,
    }
}

fn task_result_from_document(payload: DocumentCheckResult) -> HarnessTaskResult {
    let artifacts = vec![HarnessArtifact::DocumentCheck {
        report: payload.clone(),
    }];
    let artifact_wires = artifacts_to_wires(&artifacts, "document");
    HarnessTaskResult {
        request_id: payload.request_id.clone(),
        run_status: HarnessRunStatus::Completed,
        artifacts,
        artifact_wires,
        evidence_packet_ids: vec![],
        usage: None,
        evidence_refresh_notice: None,
        chat_payload: None,
        writing: None,
        citation: None,
        organize: None,
        research: None,
        chapter: None,
        document: Some(payload),
    }
}

fn artifacts_to_wires(
    artifacts: &[HarnessArtifact],
    source_task: &str,
) -> Vec<HarnessArtifactWire> {
    artifacts
        .iter()
        .map(|a| {
            let (kind, title, payload) = match a {
                HarnessArtifact::Message { content, .. } => {
                    ("message", "回答", serde_json::json!({ "content": content }))
                }
                HarnessArtifact::Patches { patches } => (
                    "patches",
                    "写作补丁",
                    serde_json::to_value(patches).unwrap_or_default(),
                ),
                HarnessArtifact::CitationReport { report } => (
                    "citation_report",
                    "引用检查",
                    serde_json::to_value(report).unwrap_or_default(),
                ),
                HarnessArtifact::OrganizeReport { report } => (
                    "organize_report",
                    "整理建议",
                    serde_json::to_value(report).unwrap_or_default(),
                ),
                HarnessArtifact::ResearchReport { payload } => {
                    ("research_report", "研究报告", payload.clone())
                }
                HarnessArtifact::DocumentCheck { report } => (
                    "document_check",
                    "文档检查",
                    serde_json::to_value(report).unwrap_or_default(),
                ),
                HarnessArtifact::ChapterWriting { payload } => {
                    ("chapter_writing", "章节写作", payload.clone())
                }
                HarnessArtifact::ToolConfirmation { .. } => {
                    ("tool_confirmation", "工具确认", serde_json::Value::Null)
                }
                HarnessArtifact::LegacyPayload { intent, json } => {
                    (intent.as_str(), "任务结果", json.clone())
                }
            };
            HarnessArtifactWire {
                kind: kind.into(),
                title: title.into(),
                status: "ready".into(),
                source_task: source_task.into(),
                // TODO: populate from actual evidence packets when HarnessArtifact carries this data
                evidence_count: 0,
                payload,
            }
        })
        .collect()
}

pub fn map_task_result_to_response(
    result: HarnessTaskResult,
) -> crate::commands::assistant_commands::AssistantExecuteResponse {
    use crate::commands::assistant_commands::AssistantExecuteResponse;
    let body = task_result_to_body(&result);
    AssistantExecuteResponse {
        body,
        request_id: result.request_id,
        run_status: run_status_wire(result.run_status).to_string(),
        evidence_refresh_notice: result.evidence_refresh_notice,
        artifacts: result.artifact_wires,
        intent_detection: None,
        run_plan_summary: None,
        permission_preflight_summary: None,
    }
}

fn task_result_to_body(
    result: &HarnessTaskResult,
) -> crate::commands::assistant_commands::AssistantExecuteBody {
    use crate::commands::assistant_commands::AssistantExecuteBody;
    if let Some(payload) = &result.chat_payload {
        return AssistantExecuteBody::Chat {
            payload: payload.clone(),
        };
    }
    if let Some(payload) = &result.writing {
        return AssistantExecuteBody::Writing {
            payload: payload.clone(),
        };
    }
    if let Some(payload) = &result.citation {
        return AssistantExecuteBody::Citation {
            payload: payload.clone(),
        };
    }
    if let Some(payload) = &result.organize {
        return AssistantExecuteBody::Organize {
            payload: payload.clone(),
        };
    }
    if let Some(payload) = &result.research {
        return AssistantExecuteBody::Research {
            payload: payload.clone(),
        };
    }
    if let Some(payload) = &result.chapter {
        return AssistantExecuteBody::Chapter {
            payload: serde_json::from_value(payload.clone()).unwrap_or_else(|_| {
                crate::ai_runtime::chapter_workflow::ChapterWritingResult {
                    request_id: result.request_id.clone(),
                    suggestions: vec![],
                    patches: vec![],
                    evidence_used: vec![],
                    total_tokens: TokenUsage::default(),
                }
            }),
        };
    }
    if let Some(payload) = &result.document {
        return AssistantExecuteBody::Document {
            payload: payload.clone(),
        };
    }
    AssistantExecuteBody::Chat {
        payload: serde_json::json!({
            "request_id": result.request_id,
            "status": "completed",
            "content": "",
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_pending_task_status() {
        let payload = serde_json::json!({
            "request_id": "r1",
            "status": "pending_tools",
            "content": "partial",
            "pending_confirmation": true,
            "tool_calls": [{"id": "tc1"}],
            "evidence_packets": [],
        });
        let r = task_result_from_chat(payload);
        assert_eq!(r.run_status, HarnessRunStatus::PendingConfirmation);
    }
}
