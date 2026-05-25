use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Keyring error: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("Embedding error: {0}")]
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
        serializer.serialize_str(&self.to_string())
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
    fn io_error_serializes() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err = AppError::Io(io);
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("file missing"));
    }

    #[test]
    fn db_error_serializes() {
        let db_err = rusqlite::Error::InvalidParameterName("bad".into());
        let err = AppError::Db(db_err);
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("Database error"));
    }

    #[test]
    fn json_error_serializes() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err = AppError::Json(json_err);
        let serialized = serde_json::to_string(&err).unwrap();
        assert!(serialized.contains("JSON error"));
    }
}
