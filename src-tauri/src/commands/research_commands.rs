//! Research workflow IPC commands — Phase 4 semi-autonomous research.
//!
//! Exposes the L3 research pipeline with per-round progress events,
//! abort/pause support, and research note generation.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::ai_runtime::research_state::ResearchState;
use crate::ai_runtime::research_workflow::{
    execute_research, ArgumentChain, EvidenceCoverageMatrix, EvidenceMatrix, ResearchConfig,
};
use crate::ai_runtime::trace::{TraceRecorder, TraceStatus};
use crate::ai_runtime::{
    AiScene, ResearchNoteRequest, ResearchNoteResult, ResearchProgress, ResearchTaskState,
    TokenUsage,
};
use crate::app::AppState;
use crate::error::AppResult;
use serde::Serialize;
use tauri::{Emitter, State};

#[derive(Debug, Clone, Serialize)]
pub struct ResearchTraceSummary {
    pub request_id: String,
    pub status: String,
    pub latency_ms: Option<u64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResearchStatusResponse {
    pub recent_research: Vec<ResearchTraceSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResearchExecuteResponse {
    pub request_id: String,
    pub topic: String,
    pub rounds: usize,
    pub evidence_matrix: EvidenceMatrix,
    pub coverage_matrix: EvidenceCoverageMatrix,
    pub argument_chain: ArgumentChain,
    pub summary: String,
    pub total_tokens: TokenUsage,
    pub research_state: ResearchState,
}

struct ActiveResearchGuard<'a> {
    active_research: &'a Mutex<HashMap<String, Arc<AtomicBool>>>,
    request_id: String,
}

impl<'a> ActiveResearchGuard<'a> {
    fn new(
        active_research: &'a Mutex<HashMap<String, Arc<AtomicBool>>>,
        request_id: String,
    ) -> Self {
        Self {
            active_research,
            request_id,
        }
    }
}

impl Drop for ActiveResearchGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.active_research.lock() {
            guard.remove(&self.request_id);
        }
    }
}

/// Execute a full research workflow (shared by IPC and assistant facade).
pub(crate) async fn execute_research_task(
    state: &AppState,
    app_handle: &tauri::AppHandle,
    topic: String,
    web_authorized: Option<bool>,
) -> AppResult<ResearchExecuteResponse> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let scene = AiScene::ResearchSynthesis;

    // Start trace
    TraceRecorder::start(&state.db, &request_id, scene)?;

    // Register cancel token
    let cancel_token = Arc::new(AtomicBool::new(false));
    {
        let mut guard = state
            .ai
            .active_research
            .lock()
            .map_err(|_| crate::error::AppError::msg("Lock error"))?;
        guard.insert(request_id.clone(), cancel_token.clone());
    }
    let _active_research_guard =
        ActiveResearchGuard::new(&state.ai.active_research, request_id.clone());

    let task_policy = crate::ai_runtime::agent_task_policy::AgentTaskPolicy::from_input(
        crate::ai_runtime::agent_task_policy::AgentTaskPolicyInput {
            intent: crate::ai_types::AgentIntent::Research,
            task_kind: crate::ai_runtime::agent_task::AgentTaskKind::Complex,
            scope: crate::ai_runtime::agent_task_policy::AgentTaskScope::Vault,
            web_authorized: web_authorized.unwrap_or(false),
            has_attachments: false,
            write_permission_required: false,
            research_depth: 2,
        },
    );
    let route =
        crate::ai_runtime::agent_task_policy::resolve_for_task_policy(&state.db, &task_policy)?;
    let provider_config = route
        .resolved
        .to_provider_config_for_slot(route.summary.slot);

    let config = ResearchConfig {
        web_research_authorized: web_authorized.unwrap_or(false),
        ..Default::default()
    };

    // Emit planning state
    let _ = app_handle.emit(
        "ai:research_progress",
        &ResearchProgress {
            request_id: request_id.clone(),
            topic: topic.clone(),
            state: ResearchTaskState::Planning,
            current_round: 0,
            max_rounds: config.max_rounds,
            queries_executed: vec![],
            new_evidence_count: 0,
            total_evidence_count: 0,
            tokens_used: 0,
            token_budget: config.token_budget,
            progress_pct: 0.0,
            round_terminated_early: false,
        },
    );

    // Execute research workflow
    let result = execute_research(
        &state.db,
        app_handle,
        &request_id,
        &topic,
        config.clone(),
        provider_config,
        web_authorized.unwrap_or(false),
        Some(cancel_token.clone()),
    )
    .await?;

    // Complete trace
    TraceRecorder::complete(
        &state.db,
        &request_id,
        TraceStatus::Completed,
        Some("reasoner"),
        Some("research_workflow"),
        None,
        None,
        None,
        Some(result.total_tokens.prompt_tokens),
        Some(result.total_tokens.completion_tokens),
        None,
    )?;

    // Emit completion progress
    let _ = app_handle
        .emit(
            "ai:research_progress",
            &ResearchProgress {
                request_id: request_id.clone(),
                topic: topic.clone(),
                state: ResearchTaskState::Completed,
                current_round: result.rounds.len() as u32,
                max_rounds: config.max_rounds,
                queries_executed: vec![],
                new_evidence_count: 0,
                total_evidence_count: result.evidence_matrix.total_evidence_count,
                tokens_used: result.total_tokens.total_tokens,
                token_budget: config.token_budget,
                progress_pct: 1.0,
                round_terminated_early: false,
            },
        )
        .ok();

    // Emit completion event (backward compat)
    app_handle
        .emit(
            "ai:research_complete",
            &serde_json::json!({
                "request_id": request_id,
                "propositions": result.evidence_matrix.propositions.len(),
                "evidence_count": result.evidence_matrix.total_evidence_count,
                "coverage_score": result.evidence_matrix.coverage_score,
            }),
        )
        .ok();

    Ok(ResearchExecuteResponse {
        request_id: result.request_id,
        topic: result.topic,
        rounds: result.rounds.len(),
        evidence_matrix: result.evidence_matrix,
        coverage_matrix: result.coverage_matrix,
        argument_chain: result.argument_chain,
        summary: result.summary,
        total_tokens: result.total_tokens,
        research_state: result.research_state,
    })
}

/// Execute a full research workflow on a topic with per-round progress events.
#[tauri::command]
pub async fn research_execute(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    topic: String,
    web_authorized: Option<bool>,
) -> AppResult<ResearchExecuteResponse> {
    execute_research_task(&state, &app_handle, topic, web_authorized).await
}

/// Abort a running research task by its request_id.
#[tauri::command]
pub fn research_abort(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    request_id: String,
) -> AppResult<()> {
    let mut guard = state
        .ai
        .active_research
        .lock()
        .map_err(|_| crate::error::AppError::msg("Lock error"))?;

    if let Some(token) = guard.get(&request_id) {
        token.store(true, Ordering::Relaxed);
        guard.remove(&request_id);

        // Update trace
        let _ = TraceRecorder::update_status(&state.db, &request_id, TraceStatus::Aborted);

        // Emit aborted progress
        let _ = app_handle.emit(
            "ai:research_progress",
            &ResearchProgress {
                request_id: request_id.clone(),
                topic: String::new(),
                state: ResearchTaskState::Aborted,
                current_round: 0,
                max_rounds: 0,
                queries_executed: vec![],
                new_evidence_count: 0,
                total_evidence_count: 0,
                tokens_used: 0,
                token_budget: 0,
                progress_pct: 0.0,
                round_terminated_early: false,
            },
        );

        Ok(())
    } else {
        Err(crate::error::AppError::msg("No active research task found"))
    }
}

/// List all active research task IDs.
#[tauri::command]
pub fn research_active_tasks(state: State<'_, Arc<AppState>>) -> AppResult<Vec<String>> {
    let guard = state
        .ai
        .active_research
        .lock()
        .map_err(|_| crate::error::AppError::msg("Lock error"))?;
    Ok(guard.keys().cloned().collect())
}

/// Generate a structured research note from research results.
#[tauri::command]
pub fn research_generate_note(
    _state: State<'_, Arc<AppState>>,
    request: ResearchNoteRequest,
) -> AppResult<ResearchNoteResult> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S");
    let date_simple = chrono::Utc::now().format("%Y-%m-%d");

    // Generate file-safe title
    let safe_title = request
        .topic
        .chars()
        .map(|c| {
            if c == '/'
                || c == '\\'
                || c == ':'
                || c == '*'
                || c == '?'
                || c == '"'
                || c == '<'
                || c == '>'
                || c == '|'
            {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string();
    let suggested_path = format!("研究笔记/{safe_title}.md");

    let section_count = 5;
    let content = format!(
        r#"---
title: "研究：{topic}"
created: "{now}"
evidence_count: {evidence_count}
coverage_score: {coverage_score:.2}
---

# 研究：{topic}

> 研究日期：{date_simple}
> 证据条目：{evidence_count} 条
> 覆盖度评分：{coverage_ratio}%

## 一、研究背景

本文档由 Iris 研究助理通过多轮检索自动生成，围绕「{topic}」展开系统性的证据收集与分析。

## 二、研究摘要

{summary}

## 三、证据概览

- 总计收集 {evidence_count} 条证据
- 覆盖度评分：{coverage_ratio}%
- 证据来源包括用户笔记、法规条款、语义锚点及外部网页

## 四、后续研究方向

- 补充本地笔记以提升证据覆盖度
- 对低可信度来源进行人工核实
- 根据研究结果撰写决策建议或政策分析

## 五、参考文献

{{引用列表将在用户确认后填充}}
"#,
        topic = request.topic,
        summary = request.summary,
        evidence_count = request.evidence_count,
        coverage_score = request.coverage_score,
        coverage_ratio = (request.coverage_score * 100.0).round() as u32,
    );

    Ok(ResearchNoteResult {
        content,
        suggested_path,
        section_count,
    })
}

/// Get research workflow status for a session.
#[tauri::command]
pub fn research_status(state: State<'_, Arc<AppState>>) -> AppResult<ResearchStatusResponse> {
    let traces = crate::ai_runtime::trace::TraceRecorder::recent(&state.db, 10)?;

    let research_traces: Vec<_> = traces
        .iter()
        .filter(|t| matches!(t.scene, AiScene::ResearchSynthesis))
        .map(|t| ResearchTraceSummary {
            request_id: t.request_id.clone(),
            status: format!("{:?}", t.status),
            latency_ms: t.latency_ms,
            created_at: t.created_at.clone(),
        })
        .collect();

    Ok(ResearchStatusResponse {
        recent_research: research_traces,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_research_guard_removes_token_on_drop() {
        let active_research = Mutex::new(HashMap::new());
        active_research
            .lock()
            .unwrap()
            .insert("req-cleanup".into(), Arc::new(AtomicBool::new(false)));

        {
            let _guard = ActiveResearchGuard::new(&active_research, "req-cleanup".into());
        }

        assert!(!active_research.lock().unwrap().contains_key("req-cleanup"));
    }
}
