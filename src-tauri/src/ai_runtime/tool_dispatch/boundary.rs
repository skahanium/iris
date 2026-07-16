use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::ai_runtime::sandbox_profile::sandbox_profile_for_tool;
use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::storage::note_write::NoteWriteService;
use crate::storage::paths::is_user_note_path;

use super::ToolDispatchContext;

const MAX_EXTERNAL_TEXT_BYTES: usize = 20 * 1024 * 1024;

#[cfg(unix)]
const SENSITIVE_PREFIXES: &[&str] = &[
    "/etc/", "/usr/", "/var/", "/opt/", "/sbin/", "/bin/", "/lib/", "/lib64/", "/boot/", "/proc/",
    "/sys/", "/dev/", "/run/", "/snap/",
];

#[cfg(windows)]
const SENSITIVE_PREFIXES: &[&str] = &[
    "C:\\Windows\\",
    "C:\\Program Files\\",
    "C:\\Program Files (x86)\\",
    "C:\\ProgramData\\",
];

fn max_chars(args: &serde_json::Value, default: usize) -> usize {
    args.get("max_chars")
        .and_then(|v| v.as_u64())
        .unwrap_or(default as u64)
        .clamp(100, 60_000) as usize
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

fn arg_str<'a>(args: &'a serde_json::Value, key: &str) -> AppResult<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg(format!("missing {key}")))
}

fn reject_parent_components(path: &Path) -> AppResult<()> {
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(AppError::msg("Path traversal is not allowed"));
    }
    Ok(())
}

fn is_sensitive_system_path(path: &Path) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/").to_lowercase();
    if normalized.starts_with("/private/var/folders/") || normalized.starts_with("/var/folders/") {
        return false;
    }
    SENSITIVE_PREFIXES
        .iter()
        .any(|prefix| normalized.starts_with(&prefix.to_lowercase()))
}

fn canonical_authorized_root(root: &Path) -> AppResult<PathBuf> {
    if is_sensitive_system_path(root) {
        return Err(AppError::msg("不允许访问系统目录"));
    }
    let root = root
        .canonicalize()
        .map_err(|_| AppError::msg("authorized_root must exist"))?;
    if !root.is_dir() {
        return Err(AppError::msg("authorized_root must be a directory"));
    }
    Ok(root)
}

fn resolve_external_input(root: &Path, path: &Path) -> AppResult<PathBuf> {
    reject_parent_components(path)?;
    let root = canonical_authorized_root(root)?;
    let path = path
        .canonicalize()
        .map_err(|_| AppError::msg("source_path does not exist"))?;
    if !path.starts_with(&root) {
        return Err(AppError::msg("source_path is outside authorized_root"));
    }
    if !path.is_file() {
        return Err(AppError::msg("source_path must be a file"));
    }
    Ok(path)
}

fn resolve_external_output(root: &Path, path: &Path) -> AppResult<PathBuf> {
    std::fs::create_dir_all(root)?;
    let root = canonical_authorized_root(root)?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    reject_parent_components(&candidate)?;
    if is_sensitive_system_path(&candidate) {
        return Err(AppError::msg("不允许导出到系统目录"));
    }
    let parent = candidate
        .parent()
        .ok_or_else(|| AppError::msg("Invalid output path"))?;
    std::fs::create_dir_all(parent)?;
    let parent = parent.canonicalize()?;
    if !parent.starts_with(&root) {
        return Err(AppError::msg("dest_path is outside authorized_root"));
    }
    let file_name = candidate
        .file_name()
        .ok_or_else(|| AppError::msg("Invalid output path"))?;
    Ok(parent.join(file_name))
}

fn resolve_new_vault_note(vault: &Path, relative: &str) -> AppResult<PathBuf> {
    if !is_user_note_path(relative) || !relative.ends_with(".md") {
        return Err(AppError::msg("目标路径必须是用户 Markdown 笔记"));
    }
    let vault = vault.canonicalize()?;
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

fn write_text_atomic(path: &Path, content: &str, overwrite: bool) -> AppResult<()> {
    if path.exists() && !overwrite {
        return Err(AppError::msg("Target already exists"));
    }
    if content.len() > MAX_EXTERNAL_TEXT_BYTES {
        return Err(AppError::msg("content exceeds 20MB limit"));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content)?;
    if let Err(err) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(err.into());
    }
    Ok(())
}

pub(super) fn fs_import_to_vault_tool(
    state: &AppState,
    _ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let source = resolve_external_input(
        Path::new(arg_str(args, "authorized_root")?),
        Path::new(arg_str(args, "source_path")?),
    )?;
    let target_path = arg_str(args, "target_path")?;
    let overwrite = args
        .get("overwrite")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let content = std::fs::read_to_string(&source)?;
    if content.len() > MAX_EXTERNAL_TEXT_BYTES {
        return Err(AppError::msg("content exceeds 20MB limit"));
    }
    let vault = state.vault_path()?;
    let _ = resolve_new_vault_note(&vault, target_path)?;
    let receipt = if overwrite {
        NoteWriteService::write(state, target_path, &content)?
    } else {
        NoteWriteService::create(state, target_path, &content)?
    };
    Ok(serde_json::json!({
        "type": "fs_import_to_vault",
        "path": receipt.entry.path,
        "bytes": content.len(),
        "title": receipt.entry.title,
        "indexStatus": receipt.index_status,
    }))
}

pub(super) fn fs_export_tool(args: &serde_json::Value) -> AppResult<serde_json::Value> {
    let dest = resolve_external_output(
        Path::new(arg_str(args, "authorized_root")?),
        Path::new(arg_str(args, "dest_path")?),
    )?;
    let content = arg_str(args, "content")?;
    let overwrite = args
        .get("overwrite")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    write_text_atomic(&dest, content, overwrite)?;
    Ok(serde_json::json!({
        "type": "fs_export",
        "destPath": dest.to_string_lossy(),
        "bytes": content.len(),
    }))
}

pub(super) fn fs_write_authorized_export_tool(
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let dest = resolve_external_output(
        Path::new(arg_str(args, "authorized_root")?),
        Path::new(arg_str(args, "target_path")?),
    )?;
    let content = arg_str(args, "content")?;
    let overwrite = args
        .get("overwrite")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    write_text_atomic(&dest, content, overwrite)?;
    Ok(serde_json::json!({
        "type": "fs_write_authorized_export",
        "destPath": dest.to_string_lossy(),
        "bytes": content.len(),
    }))
}

pub(super) fn fs_read_authorized_folder_tool(
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let root = canonical_authorized_root(Path::new(arg_str(args, "authorized_root")?))?;
    let max_entries = args
        .get("max_entries")
        .and_then(|v| v.as_u64())
        .unwrap_or(100)
        .clamp(1, 500) as usize;
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&root)?.take(max_entries) {
        let entry = entry?;
        let metadata = entry.metadata()?;
        entries.push(serde_json::json!({
            "name": entry.file_name().to_string_lossy(),
            "kind": if metadata.is_dir() { "directory" } else { "file" },
            "bytes": if metadata.is_file() { metadata.len() } else { 0 },
        }));
    }
    Ok(serde_json::json!({
        "type": "fs_read_authorized_folder",
        "root": root.to_string_lossy(),
        "entries": entries,
        "count": entries.len(),
    }))
}

pub(super) fn doc_normalize_markdown_tool(
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let content = arg_str(args, "content")?;
    let markdown = normalize_markdown(content);
    Ok(serde_json::json!({
        "type": "doc_normalize_markdown",
        "markdown": markdown,
        "charCount": markdown.chars().count(),
    }))
}

pub(super) fn doc_extract_citations_tool(args: &serde_json::Value) -> AppResult<serde_json::Value> {
    let content = arg_str(args, "content")?;
    let citations: Vec<serde_json::Value> = extract_urls(content)
        .into_iter()
        .map(|url| serde_json::json!({ "url": url }))
        .collect();
    Ok(serde_json::json!({
        "type": "doc_extract_citations",
        "citations": citations,
        "count": citations.len(),
    }))
}

fn normalize_markdown(content: &str) -> String {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::new();
    let mut blank_count = 0usize;
    let mut fence: Option<(char, usize)> = None;
    for line in normalized.lines() {
        if let Some((marker, marker_len)) = fence {
            out.push_str(line);
            out.push('\n');
            if let Some((close_marker, close_len)) = markdown_fence_marker(line) {
                if close_marker == marker && close_len >= marker_len {
                    fence = None;
                    blank_count = 0;
                }
            }
            continue;
        }

        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            blank_count += 1;
            if blank_count <= 1 {
                out.push('\n');
            }
            continue;
        }
        blank_count = 0;
        out.push_str(trimmed);
        out.push('\n');
        if let Some(marker) = markdown_fence_marker(trimmed) {
            fence = Some(marker);
        }
    }
    while out.starts_with('\n') {
        out.remove(0);
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn markdown_fence_marker(line: &str) -> Option<(char, usize)> {
    let leading_spaces = line.chars().take_while(|c| *c == ' ').count();
    if leading_spaces > 3 {
        return None;
    }
    let rest = &line[leading_spaces..];
    let marker = rest.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let len = rest.chars().take_while(|c| *c == marker).count();
    (len >= 3).then_some((marker, len))
}

fn extract_urls(content: &str) -> Vec<String> {
    let mut urls = Vec::new();
    for token in content.split_whitespace() {
        let url = token
            .trim_matches(|c: char| {
                matches!(
                    c,
                    '<' | '>' | '(' | ')' | '[' | ']' | ',' | '.' | ';' | '"' | '\''
                )
            })
            .to_string();
        if (url.starts_with("https://") || url.starts_with("http://")) && !urls.contains(&url) {
            urls.push(url);
        }
    }
    urls
}

fn run_git(state: &AppState, args: &[&str], max: usize) -> AppResult<String> {
    let vault = state.vault_path()?;
    let output = Command::new("git")
        .args([
            "-c",
            "core.quotepath=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "filter.lfs.smudge=",
            "-c",
            "filter.lfs.required=false",
        ])
        .args(args)
        .current_dir(&vault)
        .env_clear()
        .env("LANG", "C")
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::msg(format!(
            "git command failed: {}",
            truncate_chars(stderr.trim(), 400)
        )));
    }
    Ok(truncate_chars(
        String::from_utf8_lossy(&output.stdout).trim(),
        max,
    ))
}

fn validate_vault_relative_path(relative: &str) -> AppResult<()> {
    reject_parent_components(Path::new(relative))?;
    if Path::new(relative).is_absolute() {
        return Err(AppError::msg("absolute paths are not allowed"));
    }
    if !is_user_note_path(relative) {
        return Err(AppError::msg("内部元数据路径不允许用于此工具"));
    }
    Ok(())
}

fn string_array_arg(args: &serde_json::Value, key: &str) -> AppResult<Vec<String>> {
    let Some(values) = args.get(key).and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| AppError::msg(format!("{key} entries must be strings")))
        })
        .collect()
}

pub(super) fn git_write_commit_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let message = arg_str(args, "message")?.trim();
    if message.is_empty() || message.len() > 500 {
        return Err(AppError::msg("commit message must be 1..500 bytes"));
    }
    let paths = string_array_arg(args, "paths")?;
    if paths.is_empty() {
        return Err(AppError::msg("paths are required"));
    }
    for path in &paths {
        validate_vault_relative_path(path)?;
    }
    let vault = state.vault_path()?;
    for path in &paths {
        let output = Command::new("git")
            .args([
                "-c",
                "core.quotepath=false",
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "filter.lfs.smudge=",
                "-c",
                "filter.lfs.required=false",
                "add",
                "--",
                path,
            ])
            .current_dir(&vault)
            .env_clear()
            .env("LANG", "C")
            .output()?;
        if !output.status.success() {
            return Err(AppError::msg("git add failed"));
        }
    }
    let output = Command::new("git")
        .args([
            "-c",
            "core.quotepath=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "filter.lfs.smudge=",
            "-c",
            "filter.lfs.required=false",
            "-c",
            "user.name=Iris Agent",
            "-c",
            "user.email=iris-agent@example.invalid",
            "commit",
            "-m",
            message,
        ])
        .current_dir(&vault)
        .env_clear()
        .env("LANG", "C")
        .output()?;
    if !output.status.success() {
        return Err(AppError::msg(format!(
            "git commit failed: {}",
            truncate_chars(String::from_utf8_lossy(&output.stderr).trim(), 400)
        )));
    }
    let commit = run_git(state, &["rev-parse", "--short", "HEAD"], 200)?;
    Ok(serde_json::json!({
        "type": "git_write_commit",
        "commit": commit,
        "paths": paths,
        "sandbox_profile": sandbox_profile_for_tool("git_write_commit"),
    }))
}

pub(super) fn git_read_status_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let status = run_git(
        state,
        &["status", "--short", "--branch"],
        max_chars(args, 12_000),
    )?;
    Ok(serde_json::json!({
        "type": "git_read_status",
        "scope": "vault",
        "status": status,
        "sandbox_profile": sandbox_profile_for_tool("git_read_status"),
    }))
}

pub(super) fn git_read_diff_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let include_patch = args
        .get("include_patch")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let git_args: &[&str] = if include_patch {
        &["diff", "--", "."]
    } else {
        &["diff", "--stat", "--", "."]
    };
    let diff = run_git(state, git_args, max_chars(args, 12_000))?;
    Ok(serde_json::json!({
        "type": "git_read_diff",
        "scope": "vault",
        "includePatch": include_patch,
        "diff": diff,
        "sandbox_profile": sandbox_profile_for_tool("git_read_diff"),
    }))
}

pub(super) fn git_read_log_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20)
        .clamp(1, 50)
        .to_string();
    let log = run_git(
        state,
        &["log", "--oneline", "--decorate", "-n", &limit],
        max_chars(args, 12_000),
    )?;
    Ok(serde_json::json!({
        "type": "git_read_log",
        "scope": "vault",
        "limit": limit,
        "log": log,
        "sandbox_profile": sandbox_profile_for_tool("git_read_log"),
    }))
}

pub(super) fn secret_exists_tool(
    _state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let service = args
        .get("service")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::msg("service is required"))?;
    crate::security::ipc_policy::validate_credential_service(service)?;
    Ok(serde_json::json!({
        "type": "secret_exists",
        "service": service,
        "exists": crate::credentials::credential_available(service)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::normalize_markdown;

    #[test]
    fn normalize_markdown_preserves_blank_lines_inside_fenced_code() {
        let input = "前文\r\n\r\n```ts\r\nconst a = 1;\r\n\r\n\r\nconst b = 2;\r\n```\r\n\r\n后文";

        let normalized = normalize_markdown(input);

        assert!(normalized.contains("const a = 1;\n\n\nconst b = 2;"));
        assert_eq!(
            normalized,
            "前文\n\n```ts\nconst a = 1;\n\n\nconst b = 2;\n```\n\n后文\n",
        );
    }

    #[test]
    fn normalize_markdown_collapses_excess_blank_lines_outside_fences() {
        let input = "\n\nAlpha\n\n\n\nBeta\n";

        assert_eq!(normalize_markdown(input), "Alpha\n\nBeta\n");
    }
}
