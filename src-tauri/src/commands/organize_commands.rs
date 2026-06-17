//! Organize workflow IPC commands.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::ai_runtime::organize_workflow::{self, FileMetadata};
use crate::ai_runtime::trace::{TraceRecorder, TraceStatus};
use crate::ai_runtime::{OrganizeTaskInput, OrganizeTaskResult};
use crate::app::AppState;
use crate::error::AppResult;

/// Execute an organize task.
///
/// This command:
/// 1. Retrieves file metadata from the vault
/// 2. Analyzes each file's title, tags, folder, links
/// 3. Generates organize suggestions (rule-based)
/// 4. Returns a batch change plan
pub(crate) async fn execute_organize_task(
    state: &AppState,
    app_handle: &AppHandle,
    input: OrganizeTaskInput,
) -> AppResult<OrganizeTaskResult> {
    let request_id = uuid::Uuid::new_v4().to_string();

    // Start trace
    TraceRecorder::start(
        &state.db,
        &request_id,
        crate::ai_runtime::AiScene::KnowledgeLookup,
    )?;

    // Retrieve file metadata
    let files = retrieve_file_metadata(state, &input)?;

    // Execute organize task
    let result = organize_workflow::execute_organize_with_metadata(&input, files)?;

    // Complete trace
    let _ = TraceRecorder::complete(
        &state.db,
        &request_id,
        TraceStatus::Completed,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );

    // Emit event to frontend
    let _ = app_handle.emit("ai:organize_complete", &request_id);

    Ok(result)
}

/// Execute an organize task.
#[tauri::command]
pub async fn organize_execute(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    input: OrganizeTaskInput,
) -> AppResult<OrganizeTaskResult> {
    execute_organize_task(state.inner().as_ref(), &app_handle, input).await
}

/// Retrieve file metadata for organize analysis.
fn retrieve_file_metadata(
    state: &AppState,
    input: &OrganizeTaskInput,
) -> AppResult<Vec<FileMetadata>> {
    let _vault = state.vault_path()?;

    let files = state.db.with_conn(|conn| {
        let mut query = String::from(
            "SELECT f.path, f.title, f.content_hash, f.word_count,
                    GROUP_CONCAT(t.name, ',') as tags
             FROM files f
             LEFT JOIN file_tags ft ON f.id = ft.file_id
             LEFT JOIN tags t ON ft.tag_id = t.id
             WHERE f.path NOT LIKE '.iris/%'
             GROUP BY f.id",
        );

        // Apply scope filter if provided
        if let Some(ref scope) = input.scope {
            if !scope.paths.is_empty() || !scope.path_prefixes.is_empty() {
                let mut conditions = Vec::new();

                if !scope.paths.is_empty() {
                    let placeholders: Vec<String> = scope
                        .paths
                        .iter()
                        .enumerate()
                        .map(|(i, _)| format!("?{}", i + 1))
                        .collect();
                    conditions.push(format!("f.path IN ({})", placeholders.join(",")));
                }

                if !scope.path_prefixes.is_empty() {
                    let start_idx = scope.paths.len() + 1;
                    let prefix_conditions: Vec<String> = scope
                        .path_prefixes
                        .iter()
                        .enumerate()
                        .map(|(i, _)| format!("f.path LIKE ?{}", start_idx + i))
                        .collect();
                    conditions.push(format!("({})", prefix_conditions.join(" OR ")));
                }

                query.push_str(&format!(" AND ({})", conditions.join(" AND ")));
            }
        }

        query.push_str(" ORDER BY f.updated_at DESC");

        let mut stmt = conn.prepare(&query)?;

        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(ref scope) = input.scope {
            for path in &scope.paths {
                params.push(Box::new(path.clone()));
            }
            for prefix in &scope.path_prefixes {
                params.push(Box::new(format!("{}%", prefix)));
            }
        }

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            let path: String = row.get(0)?;
            let title: String = row.get(1)?;
            let content_hash: String = row.get(2)?;
            let word_count: i64 = row.get(3)?;
            let tags_str: Option<String> = row.get(4)?;

            let tags: Vec<String> = tags_str
                .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
                .unwrap_or_default();

            Ok(FileMetadata {
                path,
                title,
                tags,
                content_hash,
                word_count,
            })
        })?;

        Ok(rows.flatten().collect())
    })?;

    Ok(files)
}

/// Apply selected organize suggestions (user-confirmed batch).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct OrganizeApplyRequest {
    pub suggestions: Vec<crate::ai_runtime::OrganizeSuggestion>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OrganizeApplyResult {
    pub applied: Vec<String>,
    pub skipped: Vec<String>,
    pub errors: Vec<String>,
}

fn read_vault_file(state: &Arc<AppState>, path: &str) -> AppResult<String> {
    let vault = state.vault_path()?;
    let abs = crate::storage::paths::resolve_vault_path(&vault, path)?;
    Ok(std::fs::read_to_string(abs)?)
}

fn write_vault_file(state: &Arc<AppState>, path: &str, content: &str) -> AppResult<()> {
    use crate::storage::paths::is_user_note_path;

    if !is_user_note_path(path) {
        return Err(crate::error::AppError::msg(
            "只能写入用户笔记，不允许修改内部元数据路径",
        ));
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
    state.db.with_conn(|conn| {
        crate::indexer::scan::index_file_from_content(
            conn,
            &vault,
            &abs,
            content,
            &hash,
            Some(state),
        )
    })?;
    Ok(())
}

fn rename_vault_file(state: &Arc<AppState>, path: &str, new_path: &str) -> AppResult<()> {
    use crate::storage::paths::is_user_note_path;

    if !is_user_note_path(path) || !is_user_note_path(new_path) {
        return Err(crate::error::AppError::msg("只能重命名用户笔记路径"));
    }
    let vault = state.vault_path()?;
    let abs = crate::storage::paths::resolve_vault_path(&vault, path)?;
    let new_abs = crate::storage::paths::resolve_vault_path(&vault, new_path)?;
    if new_abs.exists() {
        return Err(crate::error::AppError::msg("Target path already exists"));
    }
    if let Some(parent) = new_abs.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&abs, &new_abs)?;
    let content = std::fs::read_to_string(&new_abs)?;
    let hash = crate::indexer::scan::content_hash(&content);
    state.storage.write_guard.mark(new_path, &hash);
    state.db.with_conn(|conn| {
        crate::indexer::scan::rename_file_index(conn, path, new_path)?;
        crate::indexer::scan::index_file_from_content(
            conn,
            &vault,
            &new_abs,
            &content,
            &hash,
            Some(state),
        )
    })?;
    Ok(())
}

fn upsert_title_in_markdown(content: &str, title: &str) -> String {
    use crate::indexer::frontmatter::split_frontmatter;
    let escaped = title.replace('\\', "\\\\").replace('"', "\\\"");
    let (yaml, body) = split_frontmatter(content);
    match yaml {
        Some(y) => {
            let mut lines: Vec<String> = y.lines().map(str::to_string).collect();
            let mut found = false;
            for line in &mut lines {
                if line.trim_start().starts_with("title:") {
                    *line = format!("title: \"{escaped}\"");
                    found = true;
                }
            }
            if !found {
                lines.insert(0, format!("title: \"{escaped}\""));
            }
            format!("---\n{}\n---\n{body}", lines.join("\n"))
        }
        None => format!("---\ntitle: \"{escaped}\"\n---\n{content}"),
    }
}

fn upsert_tag_in_markdown(content: &str, tag: &str) -> String {
    use crate::indexer::frontmatter::split_frontmatter;
    let (yaml, body) = split_frontmatter(content);
    let tag_line = format!("tags: [{tag}]");
    match yaml {
        Some(y) => {
            let mut lines: Vec<String> = y.lines().map(str::to_string).collect();
            let mut found = false;
            for line in &mut lines {
                if line.trim_start().starts_with("tags:") {
                    if line.contains('[') {
                        if !line.contains(tag) {
                            *line = line.trim_end_matches(']').to_string() + &format!(", {tag}]");
                        }
                    } else {
                        *line = tag_line.clone();
                    }
                    found = true;
                }
            }
            if !found {
                lines.push(tag_line);
            }
            format!("---\n{}\n---\n{body}", lines.join("\n"))
        }
        None => format!("---\n{tag_line}\n---\n{content}"),
    }
}

#[tauri::command]
pub fn organize_apply(
    state: State<'_, Arc<AppState>>,
    request: OrganizeApplyRequest,
) -> AppResult<OrganizeApplyResult> {
    use crate::ai_runtime::OrganizeSuggestionType;
    let state = state.inner();
    let mut applied = Vec::new();
    let mut skipped = Vec::new();
    let mut errors = Vec::new();

    for sug in request.suggestions {
        let id = sug.id.clone();
        let result = (|| -> AppResult<()> {
            match sug.suggestion_type {
                OrganizeSuggestionType::RenameTitle => {
                    if sug.suggested_value.starts_with('[') {
                        return Err(crate::error::AppError::msg("需用户输入有效标题"));
                    }
                    let content = read_vault_file(state, &sug.target_path)?;
                    let updated = upsert_title_in_markdown(&content, &sug.suggested_value);
                    write_vault_file(state, &sug.target_path, &updated)?;
                    let suggest = crate::commands::file::path_sync_suggest_inner(
                        state,
                        sug.target_path.clone(),
                        sug.suggested_value.clone(),
                    )?;
                    if suggest.needs_sync && suggest.suggested_path != suggest.current_path {
                        rename_vault_file(state, &suggest.current_path, &suggest.suggested_path)?;
                    }
                }
                OrganizeSuggestionType::MoveToFolder => {
                    let file_name = sug
                        .target_path
                        .split('/')
                        .next_back()
                        .unwrap_or(&sug.target_path);
                    let new_path = if sug.suggested_value.is_empty() {
                        file_name.to_string()
                    } else {
                        format!(
                            "{}/{}",
                            sug.suggested_value.trim_end_matches('/'),
                            file_name
                        )
                    };
                    rename_vault_file(state, &sug.target_path, &new_path)?;
                }
                OrganizeSuggestionType::AddTag => {
                    let content = read_vault_file(state, &sug.target_path)?;
                    let updated = upsert_tag_in_markdown(&content, &sug.suggested_value);
                    write_vault_file(state, &sug.target_path, &updated)?;
                }
                _ => {
                    return Err(crate::error::AppError::msg(format!(
                        "暂不支持自动应用：{:?}",
                        sug.suggestion_type
                    )));
                }
            }
            Ok(())
        })();

        match result {
            Ok(()) => applied.push(id),
            Err(e) if e.to_string().contains("暂不支持") => skipped.push(id),
            Err(e) => errors.push(format!("{id}: {e}")),
        }
    }

    Ok(OrganizeApplyResult {
        applied,
        skipped,
        errors,
    })
}
