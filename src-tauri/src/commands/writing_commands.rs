//! Writing workflow IPC commands.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::ai_runtime::evidence_mixer;
use crate::ai_runtime::retrieval_broker::{RetrievalLayers, RetrievalRequest};
use crate::ai_runtime::retrieval_scope::RetrievalScope;
use crate::ai_runtime::trace::{TraceRecorder, TraceStatus};
use crate::ai_runtime::web_evidence_broker::{
    collect_web_evidence, web_evidence_items_to_packets, WebEvidenceBrokerInput,
};
use crate::ai_runtime::writing_state::{save_writing_state, WritingState, WritingStateInput};
use crate::ai_runtime::writing_workflow::{self, WritingTaskOutput};
use crate::ai_runtime::{
    AiScene, PatchApplyResult, PatchProposal, SourceSpan, TokenUsage, WritingIntent,
};
use crate::ai_types::{EditTarget, EditTargetPlacement};
use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::indexer::scan::FileEntry;

/// Writing task input from frontend.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct WritingTaskInputIpc {
    /// Target file relative path
    pub target_path: String,
    /// Base content hash (SHA-256)
    pub base_content_hash: String,
    /// Selected text (optional)
    pub selection: Option<String>,
    /// Cursor context (surrounding text)
    pub cursor_context: String,
    /// Writing goal
    pub writing_goal: String,
    /// Whether web search is authorized
    pub web_authorized: bool,
    /// Controlled edit target for no-selection insertion tasks.
    #[serde(default)]
    pub edit_target: Option<EditTarget>,
}

/// Apply a validated patch to a file (read → validate → write).
#[tauri::command]
pub fn patch_apply(
    state: State<'_, Arc<AppState>>,
    patch: PatchProposal,
) -> AppResult<PatchApplyResult> {
    use crate::storage::paths::is_user_note_path;

    if !is_user_note_path(&patch.target_path) {
        return Ok(PatchApplyResult {
            success: false,
            new_content_hash: None,
            error: Some("只能修改用户笔记".into()),
            warnings: vec![],
        });
    }
    let content = file_read_inner(state.inner(), &patch.target_path)?;
    let applied = match writing_workflow::apply_patch(&patch, &content) {
        Ok(c) => c,
        Err(e) => {
            return Ok(PatchApplyResult {
                success: false,
                new_content_hash: None,
                error: Some(e.to_string()),
                warnings: vec![],
            });
        }
    };
    let (entry, mut warnings) = file_write_inner(state.inner(), &patch.target_path, &applied)?;
    let hash = writing_workflow::compute_content_hash(&applied);
    warnings.insert(
        0,
        format!("已写入《{}》，共 {} 字", entry.title, entry.word_count),
    );
    Ok(PatchApplyResult {
        success: true,
        new_content_hash: Some(hash),
        error: None,
        warnings,
    })
}

fn file_read_inner(state: &Arc<AppState>, path: &str) -> AppResult<String> {
    let vault = state.vault_path()?;
    let abs = crate::storage::paths::resolve_vault_path(&vault, path)?;
    Ok(std::fs::read_to_string(abs)?)
}

fn file_write_inner(
    state: &Arc<AppState>,
    path: &str,
    content: &str,
) -> AppResult<(FileEntry, Vec<String>)> {
    use crate::error::AppError;
    use crate::storage::paths::is_user_note_path;

    if !is_user_note_path(path) {
        return Err(AppError::msg("只能写入用户笔记，不允许修改内部元数据路径"));
    }
    let vault = state.vault_path()?;
    let abs = crate::storage::paths::resolve_vault_path(&vault, path)?;
    let tmp = abs.with_extension("md.tmp");
    std::fs::write(&tmp, content)?;
    if let Err(e) = std::fs::rename(&tmp, &abs) {
        let _ = crate::security::secure_delete::secure_delete(&tmp);
        return Err(e.into());
    }

    let hash = crate::indexer::scan::content_hash(content);
    state.storage.write_guard.mark(path, &hash);

    match state.db.with_conn(|conn| {
        crate::indexer::scan::index_file_from_content(
            conn,
            &vault,
            &abs,
            content,
            &hash,
            crate::indexer::scan::IndexEmbeddingMode::Queue(state),
        )
    }) {
        Ok(entry) => Ok((entry, vec![])),
        Err(error) => {
            tracing::warn!(
                target_path = %path,
                error = %error,
                "file write succeeded but index refresh failed"
            );
            let entry = state.db.with_conn(|conn| {
                crate::indexer::scan::peek_file_entry_after_write(conn, &vault, &abs, content)
            })?;
            Ok((
                entry,
                vec!["文档已写入，但索引刷新失败。可继续编辑，稍后可重新索引。".into()],
            ))
        }
    }
}

fn file_read_inner_state(state: &AppState, path: &str) -> AppResult<String> {
    use crate::storage::paths::is_user_note_path;

    if !is_user_note_path(path) {
        return Err(AppError::msg("只能读取用户笔记，不允许读取内部元数据路径"));
    }
    let vault = state.vault_path()?;
    let abs = crate::storage::paths::resolve_vault_path(&vault, path)?;
    Ok(std::fs::read_to_string(abs)?)
}

fn validate_edit_range(content: &str, range: SourceSpan) -> AppResult<SourceSpan> {
    if range.start > range.end || range.end > content.len() {
        return Err(AppError::msg("编辑目标范围超出文档边界"));
    }
    if !content.is_char_boundary(range.start) || !content.is_char_boundary(range.end) {
        return Err(AppError::msg("编辑目标范围不在 UTF-8 字符边界上"));
    }
    Ok(range)
}

fn markdown_heading(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    let hashes = trimmed
        .as_bytes()
        .iter()
        .take_while(|b| **b == b'#')
        .count();
    if hashes == 0 || hashes > 6 || !trimmed.as_bytes().get(hashes).is_some_and(|b| *b == b' ') {
        return None;
    }
    Some((hashes, trimmed[hashes + 1..].trim()))
}

fn iter_markdown_lines_with_offsets(content: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut offset = 0usize;
    content.split_inclusive('\n').map(move |line| {
        let start = offset;
        offset += line.len();
        (start, line)
    })
}

fn insertion_after_heading(content: &str, target: &EditTarget) -> AppResult<SourceSpan> {
    let heading_text = target
        .heading_text
        .as_deref()
        .ok_or_else(|| AppError::msg("after_heading 需要 headingText"))?;
    for (start, line) in iter_markdown_lines_with_offsets(content) {
        let Some((level, text)) = markdown_heading(line) else {
            continue;
        };
        if target
            .heading_level
            .is_none_or(|expected| expected == level)
            && text == heading_text
        {
            let end = start + line.len();
            return validate_edit_range(content, SourceSpan { start: end, end });
        }
    }
    Err(AppError::msg(format!("未找到编辑目标标题：{heading_text}")))
}

fn insertion_at_heading_ordinal(content: &str, target: &EditTarget) -> AppResult<SourceSpan> {
    let level = target.heading_level.unwrap_or(1);
    let ordinal = target
        .ordinal
        .ok_or_else(|| AppError::msg("insert_heading_at_ordinal 需要 ordinal"))?;
    if ordinal == 0 {
        return Err(AppError::msg("标题序号必须从 1 开始"));
    }

    let mut seen = 0usize;
    for (start, line) in iter_markdown_lines_with_offsets(content) {
        let Some((line_level, _)) = markdown_heading(line) else {
            continue;
        };
        if line_level == level {
            seen += 1;
            if seen == ordinal {
                return validate_edit_range(content, SourceSpan { start, end: start });
            }
        }
    }

    if seen + 1 == ordinal {
        let end = content.len();
        return validate_edit_range(content, SourceSpan { start: end, end });
    }

    Err(AppError::msg(format!(
        "第 {ordinal} 个 {level} 级标题超出当前文档范围"
    )))
}

pub(crate) fn resolve_edit_target_range(
    content: &str,
    target: &EditTarget,
) -> AppResult<SourceSpan> {
    if let Some(range) = target.range.clone() {
        return validate_edit_range(content, range);
    }

    match target.placement {
        EditTargetPlacement::ReplaceSelection => {
            Err(AppError::msg("replace_selection 需要显式 range 或选区"))
        }
        EditTargetPlacement::Cursor => Err(AppError::msg(
            "cursor placement 需要显式 range；当前请求没有提供光标字节位置",
        )),
        EditTargetPlacement::AppendDocument => {
            let end = content.len();
            validate_edit_range(content, SourceSpan { start: end, end })
        }
        EditTargetPlacement::AfterHeading => insertion_after_heading(content, target),
        EditTargetPlacement::InsertHeadingAtOrdinal => {
            insertion_at_heading_ordinal(content, target)
        }
    }
}

fn read_edit_target_content(
    state: &AppState,
    input: &WritingTaskInputIpc,
    target_path: &str,
) -> AppResult<String> {
    if target_path == input.target_path && !input.cursor_context.is_empty() {
        return Ok(input.cursor_context.clone());
    }
    file_read_inner_state(state, target_path)
}

fn insertion_fallback_text(target: &EditTarget, goal: &str) -> String {
    let body = goal.trim();
    match target.placement {
        EditTargetPlacement::InsertHeadingAtOrdinal => {
            let level = target.heading_level.unwrap_or(1).clamp(1, 6);
            let heading = target.heading_text.as_deref().unwrap_or("新增内容");
            if body.is_empty() {
                format!(
                    "{} {}

",
                    "#".repeat(level),
                    heading
                )
            } else {
                format!(
                    "{} {}

{}
",
                    "#".repeat(level),
                    heading,
                    body
                )
            }
        }
        _ if body.is_empty() => "
"
        .to_string(),
        _ => format!(
            "
{}
",
            body
        ),
    }
}

fn normalize_insertion_text(target: &EditTarget, draft: String) -> String {
    let trimmed = draft.trim();
    if matches!(
        target.placement,
        EditTargetPlacement::InsertHeadingAtOrdinal
    ) {
        let level = target.heading_level.unwrap_or(1).clamp(1, 6);
        let heading = target.heading_text.as_deref().unwrap_or("新增内容");
        if trimmed.starts_with('#') {
            format!(
                "{}

",
                trimmed
            )
        } else if trimmed.is_empty() {
            format!(
                "{} {}

",
                "#".repeat(level),
                heading
            )
        } else {
            format!(
                "{} {}

{}
",
                "#".repeat(level),
                heading,
                trimmed
            )
        }
    } else if trimmed.is_empty() {
        "
"
        .to_string()
    } else {
        format!(
            "
{}
",
            trimmed
        )
    }
}

/// Execute a writing task (shared by IPC command and assistant facade).
pub(crate) async fn execute_writing_task(
    state: &AppState,
    app_handle: &AppHandle,
    input: WritingTaskInputIpc,
) -> AppResult<WritingTaskOutput> {
    let request_id = uuid::Uuid::new_v4().to_string();

    TraceRecorder::start(&state.db, &request_id, AiScene::DraftingAssist)?;

    let intent =
        writing_workflow::detect_writing_intent(&input.writing_goal, input.selection.as_deref());

    let mut evidence = retrieve_writing_evidence(state, &input).await?;

    if input.web_authorized {
        let query = format!(
            "{} {}",
            input.writing_goal,
            input.selection.as_deref().unwrap_or("")
        );
        if let Ok(items) = collect_web_evidence(
            &state.db,
            WebEvidenceBrokerInput {
                query: query.trim().to_string(),
                urls: Vec::new(),
                enabled: input.web_authorized,
                max_search_results: 8,
                max_fetches: 3,
            },
        )
        .await
        {
            let web = web_evidence_items_to_packets(&input.writing_goal, &items);
            evidence = evidence_mixer::mix_and_rank(evidence, web, 20);
        }
    }

    let task_intent = if input.selection.as_ref().is_some_and(|s| !s.is_empty()) {
        crate::ai_types::AgentIntent::RewriteSelection
    } else if matches!(intent, WritingIntent::AddEvidence) {
        crate::ai_types::AgentIntent::CitationCheck
    } else {
        crate::ai_types::AgentIntent::Write
    };
    let task_policy = crate::ai_runtime::agent_task_policy::AgentTaskPolicy::from_input(
        crate::ai_runtime::agent_task_policy::AgentTaskPolicyInput {
            intent: task_intent,
            task_kind: crate::ai_runtime::agent_task::AgentTaskKind::Lightweight,
            scope: if input.selection.as_ref().is_some_and(|s| !s.is_empty()) {
                crate::ai_runtime::agent_task_policy::AgentTaskScope::Selection
            } else {
                crate::ai_runtime::agent_task_policy::AgentTaskScope::Note
            },
            web_authorized: input.web_authorized,
            has_attachments: false,
            write_permission_required: true,
            research_depth: matches!(task_intent, crate::ai_types::AgentIntent::CitationCheck)
                as u32,
        },
    );
    let route =
        crate::ai_runtime::agent_task_policy::resolve_for_task_policy(&state.db, &task_policy)?;
    let provider_config = route
        .resolved
        .to_provider_config_for_slot(route.summary.slot);

    let suggestion = match &intent {
        WritingIntent::Continue => {
            writing_workflow::build_writing_suggestion(intent.clone(), "基于选区与证据续写", 0.85)
        }
        WritingIntent::Rewrite => {
            writing_workflow::build_writing_suggestion(intent.clone(), "改写选中文本", 0.85)
        }
        WritingIntent::AddEvidence => {
            writing_workflow::build_writing_suggestion(intent.clone(), "补充引用依据", 0.8)
        }
        WritingIntent::Outline => {
            writing_workflow::build_writing_suggestion(intent.clone(), "生成提纲", 0.75)
        }
        WritingIntent::UnifyTone => {
            writing_workflow::build_writing_suggestion(intent.clone(), "统一语气", 0.8)
        }
        _ => writing_workflow::build_writing_suggestion(intent.clone(), "写作建议", 0.75),
    };

    let evidence_ids: Vec<String> = evidence.iter().map(|p| p.id.clone()).collect();
    let mut total_tokens = TokenUsage::default();

    let patches = if let Some(ref selection) = input.selection {
        if !selection.is_empty() {
            let range = find_selection_range(&input.cursor_context, selection);
            let (replacement, usage) = writing_workflow::generate_replacement_with_llm(
                &state.db,
                app_handle,
                &provider_config,
                &intent,
                selection,
                &input.cursor_context,
                &input.writing_goal,
                &evidence,
            )
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("Writing LLM failed: {e}");
                (
                    fallback_replacement(&intent, selection, &input.writing_goal),
                    TokenUsage::default(),
                )
            });
            total_tokens = usage;
            vec![writing_workflow::build_patch_proposal(
                &input.target_path,
                &input.base_content_hash,
                selection,
                &replacement,
                range,
                evidence_ids.clone(),
            )]
        } else {
            vec![]
        }
    } else if let Some(ref edit_target) = input.edit_target {
        let target_path = edit_target
            .target_path
            .clone()
            .unwrap_or_else(|| input.target_path.clone());
        let target_content = read_edit_target_content(state, &input, &target_path)?;
        let range = resolve_edit_target_range(&target_content, edit_target)?;
        let original_text = target_content
            .get(range.start..range.end)
            .ok_or_else(|| AppError::msg("编辑目标范围不是有效 UTF-8"))?;
        let base_hash = edit_target
            .base_content_hash
            .as_deref()
            .filter(|hash| !hash.is_empty())
            .unwrap_or_else(|| {
                if target_path == input.target_path && !input.base_content_hash.is_empty() {
                    &input.base_content_hash
                } else {
                    ""
                }
            });
        let computed_hash;
        let base_content_hash = if base_hash.is_empty() {
            computed_hash = writing_workflow::compute_content_hash(&target_content);
            computed_hash.as_str()
        } else {
            base_hash
        };
        let (draft, usage) = writing_workflow::generate_replacement_with_llm(
            &state.db,
            app_handle,
            &provider_config,
            &intent,
            "",
            &target_content,
            &input.writing_goal,
            &evidence,
        )
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("Writing insertion LLM failed: {e}");
            (
                insertion_fallback_text(edit_target, &input.writing_goal),
                TokenUsage::default(),
            )
        });
        total_tokens = usage;
        let replacement = normalize_insertion_text(edit_target, draft);
        vec![writing_workflow::build_patch_proposal(
            &target_path,
            base_content_hash,
            original_text,
            &replacement,
            range,
            evidence_ids.clone(),
        )]
    } else {
        vec![]
    };

    let packet_ids: Vec<String> = evidence.iter().map(|p| p.id.clone()).collect();
    let _ = TraceRecorder::complete(
        &state.db,
        &request_id,
        TraceStatus::Completed,
        None,
        None,
        None,
        Some(&packet_ids),
        None,
        None,
        None,
        None,
    );

    let _ = app_handle.emit("ai:writing_complete", &request_id);
    let writing_state = WritingState::from_input(WritingStateInput {
        request_id: request_id.clone(),
        target_path: input.target_path.clone(),
        base_content_hash: input.base_content_hash.clone(),
        writing_goal: input.writing_goal.clone(),
        intent: format!("{intent:?}").to_ascii_lowercase(),
        evidence: evidence.clone(),
        patches: patches.clone(),
    });
    let _ = save_writing_state(&state.db, &writing_state);

    Ok(WritingTaskOutput {
        request_id,
        suggestions: vec![suggestion],
        patches,
        evidence_used: evidence,
        total_tokens,
        writing_state,
    })
}

/// Execute a writing task.
#[tauri::command]
pub async fn writing_execute(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    input: WritingTaskInputIpc,
) -> AppResult<WritingTaskOutput> {
    execute_writing_task(state.inner().as_ref(), &app_handle, input).await
}

fn fallback_replacement(intent: &WritingIntent, selection: &str, goal: &str) -> String {
    match intent {
        WritingIntent::Continue => format!("{selection}\n\n"),
        WritingIntent::Rewrite => selection.to_string(),
        _ => format!("{selection}\n\n<!-- {goal} -->"),
    }
}

async fn retrieve_writing_evidence(
    state: &AppState,
    input: &WritingTaskInputIpc,
) -> AppResult<Vec<crate::ai_runtime::ContextPacket>> {
    let query = format!(
        "{} {}",
        input.writing_goal,
        input.selection.as_deref().unwrap_or("")
    );

    let request = RetrievalRequest {
        query: query.trim().to_string(),
        max_results: 10,
        layers: RetrievalLayers::default(),
        note_context: Some(input.target_path.clone()),
        file_id_context: None,
        scope: RetrievalScope::default(),
    };

    state
        .db
        .with_conn(|conn| crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request))
}

fn find_selection_range(content: &str, selection: &str) -> crate::ai_runtime::SourceSpan {
    if let Some(pos) = content.find(selection) {
        crate::ai_runtime::SourceSpan {
            start: pos,
            end: pos + selection.len(),
        }
    } else {
        crate::ai_runtime::SourceSpan {
            start: content.len(),
            end: content.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn edit_target_insert_heading_at_ordinal_resolves_utf8_boundary() {
        let content = "# 一、事实

甲乙。

# 二、分析

丙丁。

# 四、其他

戊己。
";
        let target = crate::ai_types::EditTarget {
            target_path: Some("case.md".to_string()),
            source: crate::ai_types::EditTargetSource::Conversation,
            placement: crate::ai_types::EditTargetPlacement::InsertHeadingAtOrdinal,
            heading_text: Some("三、核查思路".to_string()),
            heading_level: Some(1),
            ordinal: Some(3),
            range: None,
            base_content_hash: Some(writing_workflow::compute_content_hash(content)),
        };

        let range = resolve_edit_target_range(content, &target).unwrap();
        assert_eq!(range.start, content.find("# 四、其他").unwrap());
        assert_eq!(range.start, range.end);
        assert!(content.is_char_boundary(range.start));
    }

    #[test]
    fn edit_target_after_heading_resolves_empty_insert_range_after_section_heading() {
        let content = "# 案件办理

## 问题线索工作思路

旧内容。
";
        let target = crate::ai_types::EditTarget {
            target_path: Some("case.md".to_string()),
            source: crate::ai_types::EditTargetSource::Prompt,
            placement: crate::ai_types::EditTargetPlacement::AfterHeading,
            heading_text: Some("问题线索工作思路".to_string()),
            heading_level: Some(2),
            ordinal: None,
            range: None,
            base_content_hash: Some(writing_workflow::compute_content_hash(content)),
        };

        let range = resolve_edit_target_range(content, &target).unwrap();
        assert_eq!(
            range.start,
            "# 案件办理

## 问题线索工作思路
"
            .len()
        );
        assert_eq!(range.start, range.end);
        assert!(content.is_char_boundary(range.start));
    }

    #[test]
    fn edit_target_cursor_without_range_returns_clarifying_error() {
        let target = crate::ai_types::EditTarget {
            target_path: Some("case.md".to_string()),
            source: crate::ai_types::EditTargetSource::Prompt,
            placement: crate::ai_types::EditTargetPlacement::Cursor,
            heading_text: None,
            heading_level: None,
            ordinal: None,
            range: None,
            base_content_hash: None,
        };

        let error = resolve_edit_target_range("# 标题\n", &target).unwrap_err();
        assert!(error.to_string().contains("cursor placement"));
    }

    #[test]
    fn file_write_inner_returns_warning_when_reindex_fails() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join("note.md"), "# Old\n\nBody").unwrap();

        let state = Arc::new(AppState::new(dir.path().join("data")).unwrap());
        state.set_vault(vault.clone()).unwrap();
        state
            .db
            .with_conn(|conn| {
                conn.execute_batch("DROP TABLE chunks;")?;
                Ok(())
            })
            .unwrap();

        let (entry, warnings) = file_write_inner(&state, "note.md", "# New\n\nBody").unwrap();

        assert_eq!(
            fs::read_to_string(vault.join("note.md")).unwrap(),
            "# New\n\nBody"
        );
        assert_eq!(entry.path, "note.md");
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("索引刷新失败")));
    }
}
