//! Application errors with sanitized frontend serialization.

use serde::{ser::SerializeStruct, Serialize};
use thiserror::Error;

/// Structured provider-dispatch failure kind used for failover routing.
///
/// Prefer emitting [`AppError::Provider`] from the model gateway over free-form
/// [`AppError::Message`] strings so classifiers do not depend on English prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderErrorKind {
    /// TCP/TLS connect failure before an HTTP response.
    Connection,
    /// Request deadline exceeded.
    Timeout,
    /// HTTP 429 / explicit rate limit.
    RateLimited,
    /// HTTP 401 / invalid API key.
    Unauthorized,
    /// HTTP 403.
    Forbidden,
    /// HTTP 503 / overloaded.
    TemporarilyUnavailable,
    /// User or runtime cancellation.
    Cancelled,
    /// Other HTTP status from the provider.
    HttpStatus(u16),
    /// Provider returned an unusable body.
    InvalidResponse,
    /// Unclassified provider failure.
    Unknown,
}

impl ProviderErrorKind {
    /// Stable wire/log token for this kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Connection => "provider_connection",
            Self::Timeout => "provider_timeout",
            Self::RateLimited => "provider_rate_limited",
            Self::Unauthorized => "provider_unauthorized",
            Self::Forbidden => "provider_forbidden",
            Self::TemporarilyUnavailable => "provider_temporarily_unavailable",
            Self::Cancelled => "provider_cancelled",
            Self::HttpStatus(_) => "provider_http_status",
            Self::InvalidResponse => "provider_invalid_response",
            Self::Unknown => "provider_unknown",
        }
    }

    /// Map an HTTP status to the most specific provider kind.
    pub fn from_http_status(status: u16) -> Self {
        match status {
            401 => Self::Unauthorized,
            403 => Self::Forbidden,
            429 => Self::RateLimited,
            503 => Self::TemporarilyUnavailable,
            other => Self::HttpStatus(other),
        }
    }
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("IO error")]
    Io(#[from] std::io::Error),
    #[error("Database error")]
    Db(#[from] rusqlite::Error),
    #[error("JSON error")]
    Json(#[from] serde_json::Error),
    #[error("HTTP error")]
    Http(#[from] reqwest::Error),
    #[error("Credential error")]
    Credential(String),
    #[error("Embedding error")]
    Embed(String),
    /// Provider-facing failure with a structured kind for failover.
    #[error("{message}")]
    Provider {
        kind: ProviderErrorKind,
        message: String,
    },
    #[error("{0}")]
    Message(String),
}

impl AppError {
    pub fn msg(s: impl Into<String>) -> Self {
        Self::Message(s.into())
    }

    /// Build a structured provider error (preferred over free-form messages).
    pub fn provider(kind: ProviderErrorKind, message: impl Into<String>) -> Self {
        Self::Provider {
            kind,
            message: message.into(),
        }
    }

    /// Classify a failed `reqwest` send (no HTTP status yet) into a provider error.
    pub fn from_reqwest_transport(error: reqwest::Error) -> Self {
        let message = format!("LLM request failed: {error}");
        if error.is_timeout() {
            Self::provider(ProviderErrorKind::Timeout, message)
        } else if error.is_connect() {
            Self::provider(ProviderErrorKind::Connection, message)
        } else {
            Self::Http(error)
        }
    }

    /// Classify an HTTP error status from an LLM provider response.
    pub fn from_llm_http_status(status: reqwest::StatusCode, message: impl Into<String>) -> Self {
        Self::provider(
            ProviderErrorKind::from_http_status(status.as_u16()),
            message,
        )
    }

    fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "io",
            Self::Db(_) => "database",
            Self::Json(_) => "json",
            Self::Http(_) => "http",
            Self::Credential(_) => "credential",
            Self::Embed(_) => "embedding",
            Self::Provider { kind, .. } => kind.as_str(),
            Self::Message(_) => "message",
        }
    }

    fn sanitized_message(&self) -> String {
        match self {
            Self::Io(_) => "IO error".to_string(),
            Self::Db(_) => "Database error".to_string(),
            Self::Json(_) => "JSON error".to_string(),
            Self::Http(_) => "HTTP error".to_string(),
            Self::Credential(_) => credential_access_message().to_string(),
            Self::Embed(_) => "Embedding error".to_string(),
            Self::Provider { message, .. } => message.clone(),
            Self::Message(s) => s.clone(),
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("code", self.code())?;
        state.serialize_field("message", &self.sanitized_message())?;
        state.end()
    }
}

fn credential_access_message() -> &'static str {
    "无法访问本地加密凭据，请在 Iris 中重新输入并保存对应供应商的 API Key。"
}

fn redacted_log_detail(detail: &str) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;

    let digest = Sha256::digest(detail.as_bytes());
    let mut short_hash = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        let _ = write!(short_hash, "{byte:02x}");
    }
    format!("len={} sha256={short_hash}", detail.len())
}

/// Logs sanitized error detail for server-side tracing, while
/// [`AppError::serialize`] controls what is sent to the frontend.
pub fn log_error(error: &AppError) {
    match error {
        AppError::Io(e) => tracing::error!(kind = "io", detail = %e, "IO error"),
        AppError::Db(e) => tracing::error!(kind = "db", detail = %e, "Database error"),
        AppError::Json(e) => tracing::error!(kind = "json", detail = %e, "JSON error"),
        AppError::Http(e) => tracing::error!(kind = "http", detail = %e, "HTTP error"),
        AppError::Credential(s) => {
            tracing::error!(kind = "credential", detail = %redacted_log_detail(s), "Credential error")
        }
        AppError::Embed(s) => {
            tracing::error!(kind = "embed", detail = %redacted_log_detail(s), "Embedding error")
        }
        AppError::Provider { kind, message } => {
            tracing::error!(
                kind = kind.as_str(),
                detail = %redacted_log_detail(message),
                "Provider error"
            )
        }
        AppError::Message(s) => {
            tracing::error!(kind = "message", detail = %redacted_log_detail(s), "App error")
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_variant_serializes() {
        let err = AppError::msg("something broke");
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("something broke"));
    }

    #[test]
    fn provider_variant_serializes_kind_code_and_message() {
        let err = AppError::provider(ProviderErrorKind::RateLimited, "请求过于频繁，请稍后再试。");
        let value = serde_json::to_value(&err).unwrap();
        assert_eq!(value["code"], "provider_rate_limited");
        assert_eq!(value["message"], "请求过于频繁，请稍后再试。");
    }

    #[test]
    fn io_error_serializes_sanitized() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err = AppError::Io(io);
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("IO error"));
        assert!(!json.contains("file missing"));
    }

    #[test]
    fn db_error_serializes_sanitized() {
        let db_err = rusqlite::Error::InvalidParameterName("bad".into());
        let err = AppError::Db(db_err);
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("Database error"));
        assert!(!json.contains("bad"));
    }

    #[test]
    fn structured_error_payload_includes_stable_code() {
        let db_err = rusqlite::Error::InvalidParameterName("bad".into());
        let err = AppError::Db(db_err);
        let value = serde_json::to_value(&err).unwrap();

        assert_eq!(value["code"], "database");
        assert_eq!(value["message"], "Database error");
        assert!(!value.to_string().contains("bad"));
    }

    #[test]
    fn json_error_serializes_sanitized() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err = AppError::Json(json_err);
        let serialized = serde_json::to_string(&err).unwrap();
        assert!(serialized.contains("JSON error"));
        assert!(!serialized.contains("not json"));
    }

    #[test]
    fn credential_error_serializes_as_actionable_message() {
        let err = AppError::Credential("locked local credential store".into());
        let serialized = serde_json::to_string(&err).unwrap();
        assert!(serialized.contains("本地加密凭据"));
        assert!(serialized.contains("API Key"));
        assert!(!serialized.contains("locked"));
    }

    #[test]
    fn embed_error_serializes_sanitized() {
        let err = AppError::Embed("failed to compute embedding for file /vault/secret.md".into());
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("Embedding error"));
        assert!(!json.contains("/vault/secret.md"));
    }

    #[test]
    fn log_detail_redacts_message_contents() {
        let secret = "api_key=sk-test-123 note body /vault/secret.md";
        let summary = redacted_log_detail(secret);
        assert!(summary.contains("len="));
        assert!(summary.contains("sha256="));
        assert!(!summary.contains("sk-test-123"));
        assert!(!summary.contains("/vault/secret.md"));
        assert!(!summary.contains("note body"));
    }
}
