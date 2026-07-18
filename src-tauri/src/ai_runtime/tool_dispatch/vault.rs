use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::Serialize;
use std::path::{Component, Path, PathBuf};

use crate::app::AppState;
use crate::commands::file::is_vault_asset_path;
use crate::error::{AppError, AppResult};
use crate::indexer::scan::index_file;
use crate::storage::atomic_write::{move_file_no_replace_locked, with_vault_move_lock};
use crate::storage::note_title::title_from_path;
use crate::storage::note_write::NoteWriteService;
use crate::storage::paths::{
    is_user_note_path, read_file_lossy, resolve_vault_path, validate_user_note_relative_path,
};

use super::ToolDispatchContext;

const MAX_NOTE_FILE_BYTES: usize = 20 * 1024 * 1024;
const MAX_ASSET_BYTES: usize = 20 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LinkImpact {
    backlink_count: usize,
    modified_sources: Vec<String>,
}

pub(super) fn vault_create_note_tool(
    state: &AppState,
    _ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let target_path = args["target_path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing target_path"))?;
    if !is_user_note_path(target_path) || !target_path.ends_with(".md") {
        return Err(AppError::msg("只能创建用户 Markdown 笔记"));
    }
    let content = args["content"].as_str().unwrap_or("");
    if content.len() > MAX_NOTE_FILE_BYTES {
        return Err(AppError::msg(format!(
            "笔记内容超过 20MB 限制（{} 字节）",
            content.len()
        )));
    }

    let vault = state.vault_path()?;
    let _ = resolve_new_vault_path(&vault, target_path)?;
    let receipt = NoteWriteService::create(state, target_path, content)?;
    Ok(serde_json::json!({
        "type": "vault_create_note",
        "path": receipt.entry.path,
        "title": receipt.entry.title,
        "wordCount": receipt.entry.word_count,
        "indexStatus": receipt.index_status,
    }))
}

pub(super) fn vault_rename_move_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    with_vault_move_lock(|| vault_rename_move_tool_locked(state, ctx, args))
}

fn vault_rename_move_tool_locked(
    state: &AppState,
    _ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let path = args["path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing path"))?;
    let new_path = args["new_path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing new_path"))?;
    if !is_user_note_path(path) || !is_user_note_path(new_path) {
        return Err(AppError::msg("只能重命名用户笔记路径"));
    }
    if !new_path.ends_with(".md") {
        return Err(AppError::msg("目标路径必须是 Markdown 文件"));
    }

    let vault = state.vault_path()?;
    let abs = validate_user_note_relative_path(&vault, path)?;
    let new_abs = resolve_new_vault_path(&vault, new_path)?;
    if !abs.is_file() {
        return Err(AppError::msg("Source note does not exist"));
    }

    let impacted_sources = backlink_source_paths(state, path)?;
    let impact = LinkImpact {
        backlink_count: impacted_sources.len(),
        modified_sources: impacted_sources.clone(),
    };

    let current = read_file_lossy(&abs)?;
    // First establish the Markdown fact. A competing destination creator is
    // rejected atomically, before snapshots, backlink writes, or index work.
    move_file_no_replace_locked(&abs, &new_abs)?;
    let hash = crate::indexer::scan::content_hash(&current);
    state.storage.write_guard.mark(new_path, &hash);
    let mut index_degraded = false;
    if crate::version::create_snapshot(
        state,
        path,
        &current,
        crate::version::SnapshotParams::manual(),
    )
    .is_err()
    {
        index_degraded = true;
        tracing::warn!(
            result_code = "ai_vault_rename_snapshot_degraded",
            "AI vault rename completed while its version snapshot could not be recorded"
        );
    }

    let old_stem = title_from_path(path);
    let new_stem = title_from_path(new_path);
    let mut modified_sources = Vec::new();
    for source_path in impacted_sources {
        match rewrite_source_wikilinks(
            state,
            &vault,
            &source_path,
            path,
            new_path,
            &old_stem,
            &new_stem,
        ) {
            Ok(true) => modified_sources.push(source_path),
            Ok(false) => {}
            Err(_) => {
                index_degraded = true;
                tracing::warn!(
                    result_code = "ai_vault_rename_cascade_degraded",
                    "AI vault rename completed while a backlink cascade could not be applied"
                );
            }
        }
    }

    let destination_indexed = state
        .db
        .with_conn(|conn| {
            if crate::indexer::scan::rename_file_index(conn, path, new_path).is_err() {
                index_degraded = true;
                tracing::warn!(
                    result_code = "ai_vault_rename_index_rename_degraded",
                    "AI vault rename continued after derived index path rename degradation"
                );
            }
            index_file(conn, &vault, &new_abs)
        })
        .is_ok();
    if destination_indexed {
        state.embedding_scheduler().notify_index_committed();
    } else {
        index_degraded = true;
        tracing::warn!(
            result_code = "ai_vault_rename_index_refresh_degraded",
            "AI vault rename completed with derived index refresh degradation"
        );
    }

    for source_path in &modified_sources {
        let Ok(abs_source) = resolve_vault_path(&vault, source_path) else {
            index_degraded = true;
            continue;
        };
        if let Ok(hash) = crate::indexer::scan::file_hash(&abs_source) {
            state.storage.write_guard.mark(source_path, &hash);
        }
        let source_indexed = state
            .db
            .with_conn(|conn| index_file(conn, &vault, &abs_source))
            .is_ok();
        if source_indexed {
            state.embedding_scheduler().notify_index_committed();
        } else {
            index_degraded = true;
            NoteWriteService::schedule_index_repair(state, source_path);
        }
    }

    if index_degraded {
        NoteWriteService::schedule_index_repair(state, new_path);
    }

    Ok(serde_json::json!({
        "type": "vault_rename_move",
        "path": new_path,
        "previousPath": path,
        "linkImpact": impact,
        "indexStatus": if index_degraded { "degraded" } else { "synced" },
        "reversibleBy": "version history and rename/move back",
    }))
}

pub(super) fn vault_delete_to_trash_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let path = args["path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing path"))?;
    if !is_user_note_path(path) {
        return Err(AppError::msg("只能删除用户笔记"));
    }
    crate::recycle::trash_document(state, path)?;
    let trash_id: Option<String> = state.db.with_read_conn(|conn| {
        Ok(conn
            .query_row(
                "SELECT id FROM recycle_bin WHERE original_path = ?1 ORDER BY deleted_at DESC LIMIT 1",
                [path],
                |row| row.get(0),
            )
            .ok())
    })?;
    Ok(serde_json::json!({
        "type": "vault_delete_to_trash",
        "path": path,
        "trashId": trash_id,
        "reversibleBy": "recycle bin restore",
    }))
}

pub(super) fn vault_asset_write_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let path = args["path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing path"))?;
    let data_base64 = args["data_base64"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing data_base64"))?;
    if !is_vault_asset_path(path) {
        return Err(AppError::msg("资源路径必须位于 assets/ 下"));
    }
    let bytes = STANDARD
        .decode(data_base64.trim())
        .map_err(|e| AppError::msg(format!("无效的资源数据: {e}")))?;
    if bytes.is_empty() {
        return Err(AppError::msg("资源数据为空"));
    }
    if bytes.len() > MAX_ASSET_BYTES {
        return Err(AppError::msg("资源超过 20MB 限制"));
    }

    let vault = state.vault_path()?;
    let abs = resolve_new_vault_path(&vault, path)?;
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::storage::atomic_write::atomic_write(&abs, &bytes)?;

    Ok(serde_json::json!({
        "type": "vault_asset_write",
        "path": path,
        "bytes": bytes.len(),
    }))
}

pub(super) fn vault_version_list_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let path = args["path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing path"))?;
    let vault = state.vault_path()?;
    let _abs = validate_user_note_relative_path(&vault, path)?;
    let versions = crate::version::version_list(state, path)?;
    Ok(serde_json::json!({
        "type": "vault_version_list",
        "path": path,
        "versions": versions,
        "count": versions.len(),
    }))
}

fn backlink_source_paths(state: &AppState, target_path: &str) -> AppResult<Vec<String>> {
    state.db.with_read_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT f.path
             FROM links l
             JOIN files f ON f.id = l.source_id
             JOIN files t ON t.id = l.target_id
             WHERE t.path = ?1
               AND f.path NOT LIKE '.classified/%'
             ORDER BY f.path",
        )?;
        let rows = stmt.query_map([target_path], |row| row.get::<_, String>(0))?;
        Ok(rows.flatten().collect())
    })
}

fn resolve_new_vault_path(vault: &Path, relative: &str) -> AppResult<PathBuf> {
    let vault = vault
        .canonicalize()
        .map_err(|e| AppError::msg(format!("Invalid vault path: {e}")))?;
    let mut joined = vault.clone();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => joined.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::msg("Path traversal is not allowed"));
            }
        }
    }
    if !joined.starts_with(&vault) {
        return Err(AppError::msg("Path is outside the vault"));
    }
    Ok(joined)
}

fn rewrite_source_wikilinks(
    state: &AppState,
    vault: &std::path::Path,
    source_path: &str,
    old_path: &str,
    new_path: &str,
    old_stem: &str,
    new_stem: &str,
) -> AppResult<bool> {
    let abs = resolve_vault_path(vault, source_path)?;
    let content = read_file_lossy(&abs)?;
    let updated = rewrite_wikilinks(&content, old_path, new_path, old_stem, new_stem);
    if updated == content {
        return Ok(false);
    }

    crate::version::create_snapshot(
        state,
        source_path,
        &content,
        crate::version::SnapshotParams::manual(),
    )?;
    NoteWriteService::write(state, source_path, &updated)?;
    Ok(true)
}

fn rewrite_wikilinks(
    content: &str,
    old_path: &str,
    new_path: &str,
    old_stem: &str,
    new_stem: &str,
) -> String {
    let mut updated = content.replace(&format!("[[{old_path}]]"), &format!("[[{new_path}]]"));
    updated = updated.replace(&format!("[[{old_stem}]]"), &format!("[[{new_stem}]]"));
    if let Some(old_no_ext) = old_path.strip_suffix(".md") {
        let new_no_ext = new_path.strip_suffix(".md").unwrap_or(new_path);
        updated = updated.replace(&format!("[[{old_no_ext}]]"), &format!("[[{new_no_ext}]]"));
    }
    updated
}
