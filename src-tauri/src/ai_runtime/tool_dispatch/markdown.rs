use crate::ai_runtime::{PatchApplyResult, PatchProposal, RiskLevel, SourceSpan};
use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::storage::paths::{is_user_note_path, resolve_vault_path};

use super::ToolDispatchContext;

const MAX_NOTE_FILE_BYTES: usize = 20 * 1024 * 1024;

pub(super) fn markdown_write_patch_apply(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    tool_name: &str,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let Some(target_path) = args
        .get("target_path")
        .and_then(|v| v.as_str())
        .or(ctx.note_path)
        .map(str::to_string)
    else {
        return Ok(markdown_write_not_applied(
            tool_name,
            "missing target_path",
            args,
        ));
    };
    if !is_user_note_path(&target_path) {
        return Ok(markdown_write_not_applied(
            tool_name,
            "只能修改用户笔记",
            args,
        ));
    }
    let Some(base_content_hash) = args
        .get("base_content_hash")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
    else {
        return Ok(markdown_write_not_applied(
            tool_name,
            "missing base_content_hash",
            args,
        ));
    };
    let Some(range) = parse_source_span(args.get("range")) else {
        return Ok(markdown_write_not_applied(tool_name, "missing range", args));
    };
    let replacement_key = if tool_name == "insert_text_at_cursor" {
        "text"
    } else {
        "replacement"
    };
    let replacement = args[replacement_key]
        .as_str()
        .ok_or_else(|| AppError::msg(format!("missing {replacement_key}")))?;
    let original_text = args
        .get("original_text")
        .and_then(|v| v.as_str())
        .or_else(|| args.get("selection").and_then(|v| v.as_str()))
        .unwrap_or("");
    let patch = PatchProposal {
        id: uuid::Uuid::new_v4().to_string(),
        target_path: target_path.clone(),
        base_content_hash: base_content_hash.to_string(),
        range,
        original_text: original_text.to_string(),
        replacement_text: replacement.to_string(),
        evidence_packet_ids: vec![],
        risk_level: parse_risk_level(args.get("risk_level")),
        warnings: vec![],
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
    };
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &target_path)?;
    let current = std::fs::read_to_string(&abs)?;
    let applied = match crate::ai_runtime::writing_workflow::apply_patch(&patch, &current) {
        Ok(content) => content,
        Err(e) => {
            let result = PatchApplyResult {
                success: false,
                new_content_hash: None,
                error: Some(e.to_string()),
                warnings: vec![],
            };
            return Ok(serde_json::json!({
                "type": "patch_apply",
                "tool_name": tool_name,
                "target_path": target_path,
                "patch_id": patch.id,
                "result": result,
            }));
        }
    };
    if applied.len() > MAX_NOTE_FILE_BYTES {
        return Err(AppError::msg(format!(
            "补丁应用后内容超过 20MB 限制（{} 字节）",
            applied.len()
        )));
    }
    let current_hash = crate::indexer::scan::content_hash(&current);
    state.db.with_conn(|conn| {
        crate::indexer::scan::index_file_from_content(
            conn,
            &vault,
            &abs,
            &current,
            &current_hash,
            None,
        )
    })?;
    crate::version::create_snapshot(
        state,
        &target_path,
        &current,
        crate::version::SnapshotParams::manual(),
    )?;
    let tmp = abs.with_extension("md.tmp");
    std::fs::write(&tmp, &applied)?;
    if let Err(e) = std::fs::rename(&tmp, &abs) {
        let _ = crate::security::secure_delete::secure_delete(&tmp);
        return Err(e.into());
    }
    let hash = crate::ai_runtime::writing_workflow::compute_content_hash(&applied);
    state.storage.write_guard.mark(&target_path, &hash);
    let entry = state.db.with_conn(|conn| {
        crate::indexer::scan::index_file_from_content(conn, &vault, &abs, &applied, &hash, None)
    })?;
    let result = PatchApplyResult {
        success: true,
        new_content_hash: Some(hash),
        error: None,
        warnings: vec![format!(
            "已写入「{}」，共 {} 字",
            entry.title, entry.word_count
        )],
    };
    Ok(serde_json::json!({
        "type": "patch_apply",
        "tool_name": tool_name,
        "target_path": target_path,
        "patch_id": patch.id,
        "result": result,
    }))
}

fn markdown_write_not_applied(
    tool_name: &str,
    reason: &str,
    args: &serde_json::Value,
) -> serde_json::Value {
    let replacement_key = if tool_name == "insert_text_at_cursor" {
        "text"
    } else {
        "replacement"
    };
    let replacement_len = args
        .get(replacement_key)
        .and_then(|v| v.as_str())
        .map(|s| s.chars().count())
        .unwrap_or(0);
    serde_json::json!({
        "type": "patch_apply",
        "tool_name": tool_name,
        "replacement_len": replacement_len,
        "result": PatchApplyResult {
            success: false,
            new_content_hash: None,
            error: Some(reason.to_string()),
            warnings: vec![],
        },
    })
}

fn parse_source_span(value: Option<&serde_json::Value>) -> Option<SourceSpan> {
    let value = value?;
    Some(SourceSpan {
        start: value.get("start")?.as_u64()? as usize,
        end: value.get("end")?.as_u64()? as usize,
    })
}

fn parse_risk_level(value: Option<&serde_json::Value>) -> RiskLevel {
    match value.and_then(|v| v.as_str()) {
        Some("high") => RiskLevel::High,
        Some("medium") => RiskLevel::Medium,
        _ => RiskLevel::Low,
    }
}
