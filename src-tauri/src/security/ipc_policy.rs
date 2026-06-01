//! IPC input validation helpers.

use std::path::{Component, Path, PathBuf};

use crate::credentials::MINIMAX_CREDENTIAL_SERVICE;
use crate::error::{AppError, AppResult};
use crate::llm::config::SETTINGS_KEY;

/// Settings keys writable via generic `settings_set` IPC.
const ALLOWED_SETTINGS_KEYS: &[&str] = &[
    "vault_path",
    SETTINGS_KEY,
    "llm_custom_base_url",
    "llm_base_url",
    "llm_usage_last",
    "web_search_backend",
    "minimax_api_host",
    "minimax_search_model",
    "minimax_search_enabled",
];

/// Validate credential service id before keyring access.
pub fn validate_credential_service(service: &str) -> AppResult<()> {
    let canonical = if service.contains('/') {
        service.replace('/', ".")
    } else {
        service.to_string()
    };
    if canonical == MINIMAX_CREDENTIAL_SERVICE || canonical.starts_with("iris.llm.") {
        return Ok(());
    }
    Err(AppError::msg(format!(
        "不允许的凭据服务名: {service}（仅支持 iris.llm.* 与 {MINIMAX_CREDENTIAL_SERVICE}）"
    )))
}

/// Validate settings key for generic get/set IPC.
pub fn validate_settings_key(key: &str) -> AppResult<()> {
    if ALLOWED_SETTINGS_KEYS.contains(&key) {
        return Ok(());
    }
    Err(AppError::msg(format!("不允许的设置项: {key}")))
}

/// Require HTTPS for user-supplied API base URLs.
pub fn validate_https_url(url: &str) -> AppResult<()> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(AppError::msg("URL 不能为空"));
    }
    if !trimmed.starts_with("https://") {
        return Err(AppError::msg("仅允许 HTTPS URL"));
    }
    if trimmed.contains('\0') {
        return Err(AppError::msg("非法 URL"));
    }
    Ok(())
}

/// Validate remote skill install URL (HTTPS only).
pub fn validate_skill_remote_url(url: &str) -> AppResult<()> {
    validate_https_url(url)?;
    if url.contains("..") {
        return Err(AppError::msg("非法 URL"));
    }
    Ok(())
}

/// Validate git remote for skill install (no option injection).
pub fn validate_skill_git_url(repo_url: &str) -> AppResult<()> {
    let trimmed = repo_url.trim();
    if trimmed.is_empty() {
        return Err(AppError::msg("仓库 URL 不能为空"));
    }
    if trimmed.starts_with('-') {
        return Err(AppError::msg("非法 git 仓库 URL"));
    }
    if !(trimmed.starts_with("https://") || trimmed.starts_with("git@")) {
        return Err(AppError::msg("仅允许 https:// 或 git@ 形式的仓库 URL"));
    }
    Ok(())
}

/// Local skill install: SKILL.md under vault or user home only.
pub fn validate_local_skill_source(source: &Path, vault: &Path) -> AppResult<PathBuf> {
    if source.file_name().and_then(|s| s.to_str()) != Some("SKILL.md") {
        return Err(AppError::msg("本地安装路径必须是 SKILL.md 文件"));
    }
    let canon = source
        .canonicalize()
        .map_err(|e| AppError::msg(format!("无法解析路径: {e}")))?;
    for component in canon.components() {
        if matches!(component, Component::ParentDir) {
            return Err(AppError::msg("非法路径"));
        }
    }
    if let Ok(vault_canon) = vault.canonicalize() {
        if canon.starts_with(&vault_canon) {
            return Ok(canon);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        if let Ok(home_canon) = home.canonicalize() {
            if canon.starts_with(&home_canon) {
                return Ok(canon);
            }
        }
    }
    Err(AppError::msg(
        "本地 Skill 仅允许从笔记库目录或用户主目录安装",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_service_allows_llm_prefix() {
        validate_credential_service("iris.llm.deepseek").unwrap();
        validate_credential_service("iris.minimax").unwrap();
        assert!(validate_credential_service("evil.service").is_err());
    }

    #[test]
    fn https_url_rejects_http() {
        assert!(validate_https_url("http://example.com").is_err());
        validate_https_url("https://api.example.com/v1").unwrap();
    }
}
