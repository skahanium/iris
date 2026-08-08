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
    "follow_system_proxy",
    "cjk_punctuation_enabled",
];

/// Validate credential service id before local encrypted credential access.
pub fn validate_credential_service(service: &str) -> AppResult<()> {
    let service = service.trim();
    let suffix = service
        .strip_prefix("iris.llm.")
        .or_else(|| service.strip_prefix("iris.mcp."));

    if let Some(suffix) = suffix {
        if suffix.is_empty()
            || suffix.starts_with('.')
            || suffix.ends_with('.')
            || suffix.contains("..")
        {
            return Err(AppError::msg(format!(
                "不允许的凭据服务名: {service}（服务后缀不能为空）"
            )));
        }
        if suffix.bytes().all(|byte| {
            matches!(
                byte,
                b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-'
            )
        }) {
            return Ok(());
        }
        return Err(AppError::msg(format!(
            "不允许的凭据服务名: {service}（仅允许小写字母、数字、点、下划线和短横线）"
        )));
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

/// True when the URL points at a loopback host (localhost, 127.0.0.0/8, ::1),
/// where plain-HTTP endpoints such as a local Ollama server are permitted.
pub fn is_loopback_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url.trim()) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host_lower = host.to_lowercase();
    if host_lower == "localhost" || host_lower.ends_with(".localhost") {
        return true;
    }
    // The url crate returns IPv6 hosts with brackets, e.g. "[::1]".
    let bare = host_lower
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(&host_lower);
    bare.parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

/// Validate an LLM provider base URL.
///
/// Loopback endpoints (e.g. Ollama on localhost) may use plain HTTP; every
/// other endpoint must stay HTTPS. The web-fetch SSRF path keeps its own
/// strict HTTPS-only validation via [`validate_https_url`].
pub fn validate_llm_base_url(url: &str) -> AppResult<()> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(AppError::msg("URL 不能为空"));
    }
    if !is_loopback_url(trimmed) && !trimmed.starts_with("https://") {
        return Err(AppError::msg("仅允许 HTTPS URL"));
    }
    if trimmed.contains('\0') {
        return Err(AppError::msg("非法 URL"));
    }
    Ok(())
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
    fn credential_service_accepts_only_canonical_llm_and_mcp_ids() {
        validate_credential_service("iris.llm.deepseek").unwrap();
        validate_credential_service("iris.llm.custom_2").unwrap();
        validate_credential_service("iris.mcp.anysearch").unwrap();

        assert!(validate_credential_service("iris/llm/deepseek").is_err());
        assert!(validate_credential_service("iris.llm.").is_err());
        assert!(validate_credential_service("iris.llm.deepseek secret").is_err());
        assert!(validate_credential_service("evil.llm.deepseek").is_err());
    }

    #[test]
    fn https_url_rejects_http() {
        assert!(validate_https_url("http://example.com").is_err());
        validate_https_url("https://api.example.com/v1").unwrap();
    }

    #[test]
    fn llm_base_url_permits_loopback_http_and_rejects_remote_http() {
        // Loopback endpoints (a local Ollama server) may use plain HTTP.
        validate_llm_base_url("http://127.0.0.1:11434").unwrap();
        validate_llm_base_url("http://localhost:11434").unwrap();
        validate_llm_base_url("http://[::1]:11434").unwrap();
        // Remote endpoints must stay HTTPS.
        assert!(validate_llm_base_url("http://api.example.com").is_err());
        validate_llm_base_url("https://api.example.com/v1").unwrap();
        assert!(validate_llm_base_url("https://evil.com\0hidden").is_err());
    }

    #[test]
    fn loopback_url_detection_covers_localhost_forms_and_loopback_ips() {
        assert!(is_loopback_url("http://localhost:11434/v1"));
        assert!(is_loopback_url("http://localhost"));
        assert!(is_loopback_url("http://127.0.0.1:11434"));
        assert!(is_loopback_url("http://127.0.0.2:11434"));
        assert!(is_loopback_url("http://[::1]:11434"));
        assert!(!is_loopback_url("https://api.example.com/v1"));
        assert!(!is_loopback_url("https://example.com"));
        assert!(!is_loopback_url("not a url"));
        assert!(!is_loopback_url(""));
    }

    #[test]
    fn settings_key_allows_theme_and_web_search_toggle() {
        validate_settings_key("theme").unwrap();
        validate_settings_key("web_search_enabled").unwrap();
        validate_settings_key("follow_system_proxy").unwrap();
        validate_settings_key("cjk_punctuation_enabled").unwrap();
    }

    #[test]
    fn settings_key_rejects_llm_routing_generic_write() {
        assert!(validate_settings_key("llm_routing").is_err());
    }
}
