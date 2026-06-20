use serde::Serialize;
use thiserror::Error;

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
    #[error("Keyring error")]
    Keyring(#[from] keyring::Error),
    #[error("Embedding error")]
    Embed(String),
    #[error("{0}")]
    Message(String),
}

impl AppError {
    pub fn msg(s: impl Into<String>) -> Self {
        Self::Message(s.into())
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let sanitized = match self {
            Self::Io(_) => "IO error".to_string(),
            Self::Db(_) => "Database error".to_string(),
            Self::Json(_) => "JSON error".to_string(),
            Self::Http(_) => "HTTP error".to_string(),
            Self::Keyring(_) => credential_access_message().to_string(),
            Self::Embed(_) => "Embedding error".to_string(),
            Self::Message(s) => s.clone(),
        };
        serializer.serialize_str(&sanitized)
    }
}

fn credential_access_message() -> &'static str {
    "无法访问系统凭据管理器，请解锁系统钥匙串，或在设置中重新保存对应供应商的 API Key。"
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
        AppError::Keyring(e) => {
            tracing::error!(kind = "keyring", detail = %e, "Keyring error")
        }
        AppError::Embed(s) => {
            tracing::error!(kind = "embed", detail = %redacted_log_detail(s), "Embedding error")
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
    fn json_error_serializes_sanitized() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err = AppError::Json(json_err);
        let serialized = serde_json::to_string(&err).unwrap();
        assert!(serialized.contains("JSON error"));
        assert!(!serialized.contains("not json"));
    }

    #[test]
    fn keyring_error_serializes_as_actionable_credential_message() {
        let err = AppError::Keyring(keyring::Error::NoStorageAccess(Box::new(
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "locked"),
        )));
        let serialized = serde_json::to_string(&err).unwrap();
        assert!(serialized.contains("系统凭据管理器"));
        assert!(serialized.contains("API Key"));
        assert!(!serialized.contains("locked"));
    }

    #[test]
    fn embed_error_serializes_sanitized() {
        let err = AppError::Embed("failed to compute embedding for file /secret/path.md".into());
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("Embedding error"));
        assert!(!json.contains("/secret/path.md"));
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
