//! IPC input validation helpers.

use crate::error::{AppError, AppResult};

/// Settings keys writable via generic `settings_set` IPC.
const ALLOWED_SETTINGS_KEYS: &[&str] = &[
    "vault_path",
    "theme",
    "web_search_enabled",
    "web_search_provider_id",
    "llm_custom_base_url",
    "llm_base_url",
    "llm_usage_last",
];

/// Validate credential service id before keyring access.
pub fn validate_credential_service(service: &str) -> AppResult<()> {
    let canonical = if service.contains('/') {
        service.replace('/', ".")
    } else {
        service.to_string()
    };
    if canonical.starts_with("iris.llm.") || canonical.starts_with("iris.mcp.") {
        return Ok(());
    }
    Err(AppError::msg(format!(
        "不允许的凭据服务名: {service}（仅支持 iris.llm.* 与 iris.mcp.*）"
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

/// Validate LLM base URL: all LLM provider endpoints must use HTTPS.
pub fn validate_llm_base_url(url: &str) -> AppResult<()> {
    validate_https_url(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_service_allows_llm_prefix() {
        validate_credential_service("iris.llm.deepseek").unwrap();
        validate_credential_service("iris.mcp.anysearch").unwrap();
        let legacy_vendor_search = format!("iris.{}{}", "mini", "max");
        assert!(validate_credential_service(&legacy_vendor_search).is_err());
        assert!(validate_credential_service("evil.service").is_err());
    }

    #[test]
    fn https_url_rejects_http() {
        assert!(validate_https_url("http://example.com").is_err());
        validate_https_url("https://api.example.com/v1").unwrap();
    }

    #[test]
    fn llm_base_url_rejects_all_http() {
        assert!(validate_llm_base_url("http://127.0.0.1:11434").is_err());
        assert!(validate_llm_base_url("http://localhost:11434").is_err());
        assert!(validate_llm_base_url("http://[::1]:11434").is_err());
        assert!(validate_llm_base_url("http://api.example.com").is_err());
    }

    #[test]
    fn llm_base_url_allows_remote_https() {
        validate_llm_base_url("https://api.example.com/v1").unwrap();
    }

    #[test]
    fn llm_base_url_rejects_null_byte() {
        assert!(validate_llm_base_url("https://evil.com\0hidden").is_err());
    }

    #[test]
    fn settings_key_allows_theme_and_web_search_toggle() {
        validate_settings_key("theme").unwrap();
        validate_settings_key("web_search_enabled").unwrap();
    }

    #[test]
    fn settings_key_rejects_llm_routing_generic_write() {
        assert!(validate_settings_key("llm_routing").is_err());
    }
}
