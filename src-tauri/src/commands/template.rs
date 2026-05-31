use std::fs;

use serde::Serialize;
use std::sync::Arc;
use tauri::State;

use crate::app::AppState;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
pub struct TemplateInfo {
    pub name: String,
}

const BUILTIN_TEMPLATES: &[(&str, &str)] = &[
    (
        "会议纪要.md",
        "# 会议纪要\n\n**日期**：\n**参与者**：\n\n## 议题\n\n## 决议\n\n## 待办\n\n- [ ] ",
    ),
    (
        "读书笔记.md",
        "# 读书笔记\n\n**书名**：\n**作者**：\n\n## 摘要\n\n## 关键摘录\n\n> \n\n## 读后感\n",
    ),
    (
        "项目复盘.md",
        "# 项目复盘\n\n**项目名**：\n**时间线**：\n\n## 成果\n\n## 问题\n\n## 改进\n",
    ),
    (
        "每日记录.md",
        "# 每日记录\n\n**日期**：\n\n## 今日完成\n\n- \n\n## 明日计划\n\n- \n\n## 备忘\n",
    ),
];

fn templates_dir(vault: &std::path::Path) -> std::path::PathBuf {
    vault.join(".iris").join("templates")
}

/// Validate template name: no path separators, must end with .md
fn validate_template_name(name: &str) -> AppResult<String> {
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(AppError::msg("Invalid template name"));
    }
    let name = if name.ends_with(".md") {
        name.to_string()
    } else {
        format!("{}.md", name)
    };
    Ok(name)
}

fn ensure_templates(vault: &std::path::Path) -> AppResult<()> {
    let dir = templates_dir(vault);
    fs::create_dir_all(&dir)?;
    for (name, content) in BUILTIN_TEMPLATES {
        let path = dir.join(name);
        if !path.exists() {
            fs::write(&path, content)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn template_list(state: State<'_, Arc<AppState>>) -> AppResult<Vec<TemplateInfo>> {
    let vault = state.vault_path()?;
    ensure_templates(&vault)?;
    let dir = templates_dir(&vault);
    let mut templates = Vec::new();
    if dir.exists() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    templates.push(TemplateInfo {
                        name: name.to_string(),
                    });
                }
            }
        }
    }
    Ok(templates)
}

#[tauri::command]
pub fn template_create(
    state: State<'_, Arc<AppState>>,
    path: String,
    template_name: String,
) -> AppResult<crate::indexer::scan::FileEntry> {
    let vault = state.vault_path()?;
    ensure_templates(&vault)?;
    let safe_name = validate_template_name(&template_name)?;
    let tmpl_path = templates_dir(&vault).join(&safe_name);
    let content = if tmpl_path.exists() {
        fs::read_to_string(&tmpl_path)?
    } else {
        format!("# {}\n\n", path.trim_end_matches(".md"))
    };

    let abs = crate::storage::paths::resolve_vault_path(&vault, &path)?;
    if abs.exists() {
        return Err(crate::error::AppError::msg("File already exists"));
    }
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&abs, &content)?;
    state
        .db
        .with_conn(|conn| crate::indexer::scan::index_file(conn, &vault, &abs))
}

/// Read template content by name.
#[tauri::command]
pub fn template_read(state: State<'_, Arc<AppState>>, name: String) -> AppResult<String> {
    let vault = state.vault_path()?;
    ensure_templates(&vault)?;
    let name = validate_template_name(&name)?;
    let tmpl_path = templates_dir(&vault).join(&name);
    if !tmpl_path.exists() {
        return Err(AppError::msg("Template not found"));
    }
    Ok(fs::read_to_string(&tmpl_path)?)
}

/// Save/update template content. Creates the template if it doesn't exist.
#[tauri::command]
pub fn template_save(
    state: State<'_, Arc<AppState>>,
    name: String,
    content: String,
) -> AppResult<()> {
    let vault = state.vault_path()?;
    let dir = templates_dir(&vault);
    fs::create_dir_all(&dir)?;
    let name = validate_template_name(&name)?;
    let tmpl_path = dir.join(&name);
    fs::write(&tmpl_path, &content)?;
    Ok(())
}

/// Delete a template by name. Built-in templates can be deleted.
#[tauri::command]
pub fn template_delete(state: State<'_, Arc<AppState>>, name: String) -> AppResult<()> {
    let vault = state.vault_path()?;
    let name = validate_template_name(&name)?;
    let tmpl_path = templates_dir(&vault).join(&name);
    if !tmpl_path.exists() {
        return Err(AppError::msg("Template not found"));
    }
    fs::remove_file(&tmpl_path)?;
    Ok(())
}
