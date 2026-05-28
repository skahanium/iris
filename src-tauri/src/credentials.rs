use keyring::Entry;

use crate::error::{AppError, AppResult};

/// Bing Web Search 凭据 ID（与前端 `BING_SEARCH_CREDENTIAL_SERVICE` 一致）。
pub const BING_SEARCH_CREDENTIAL_SERVICE: &str = "iris.bing.search";

const KEYRING_ACCOUNT: &str = "api_key";

/// LLM 厂商凭据 ID（与前端 `llmCredentialService` 一致）。
pub fn llm_credential_service(provider: &str) -> String {
    format!("iris.llm.{provider}")
}

/// 将旧版 `iris/llm/openai` 等形式规范为 `iris.llm.openai`（Windows 凭据目标名不宜含 `/`）。
fn canonical_service_id(service: &str) -> String {
    if service.contains('/') {
        service.replace('/', ".")
    } else {
        service.to_string()
    }
}

fn entry_canonical(canonical: &str) -> AppResult<Entry> {
    Entry::new(canonical, KEYRING_ACCOUNT).map_err(Into::into)
}

fn entry_legacy(service: &str) -> AppResult<Entry> {
    Entry::new("iris", service).map_err(Into::into)
}

/// Store a secret in the OS credential manager.
pub fn set_secret(service: &str, value: &str) -> AppResult<()> {
    let canonical = canonical_service_id(service);
    entry_canonical(&canonical)?.set_password(value)?;
    tracing::debug!("credential stored for {canonical}");
    Ok(())
}

/// Read a secret from the OS credential manager.
pub fn get_secret(service: &str) -> AppResult<String> {
    let canonical = canonical_service_id(service);
    if let Ok(entry) = entry_canonical(&canonical) {
        if let Ok(password) = entry.get_password() {
            return Ok(password);
        }
    }
    if service.contains('/') {
        return entry_legacy(service)?.get_password().map_err(Into::into);
    }
    Err(AppError::msg(format!("凭据不存在: {canonical}")))
}

/// Delete a stored secret.
pub fn delete_secret(service: &str) -> AppResult<()> {
    let canonical = canonical_service_id(service);
    if let Ok(entry) = entry_canonical(&canonical) {
        let _ = entry.delete_credential();
    }
    if service.contains('/') {
        if let Ok(entry) = entry_legacy(service) {
            let _ = entry.delete_credential();
        }
    }
    Ok(())
}

/// Check if a secret exists without logging its value.
pub fn has_secret(service: &str) -> bool {
    get_secret(service).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_id_replaces_slashes() {
        assert_eq!(
            canonical_service_id("iris/llm/deepseek"),
            "iris.llm.deepseek"
        );
        assert_eq!(canonical_service_id("iris.bing.search"), "iris.bing.search");
    }

    #[test]
    #[ignore = "requires OS credential store; run locally: cargo test credential_roundtrip -- --ignored"]
    fn credential_roundtrip() {
        let id = format!("iris.test.{}", uuid::Uuid::new_v4());
        set_secret(&id, "test-secret-value").expect("set");
        assert!(has_secret(&id));
        assert_eq!(get_secret(&id).expect("get"), "test-secret-value");
        delete_secret(&id).expect("delete");
        assert!(!has_secret(&id));
    }
}
