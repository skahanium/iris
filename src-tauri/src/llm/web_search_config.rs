//! 联网检索偏好（SQLite settings，与对话路由分离）。

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::storage::db::Database;

pub const MINIMAX_API_HOST_KEY: &str = "minimax_api_host";
pub const WEB_SEARCH_BACKEND_KEY: &str = "web_search_backend";

pub const DEFAULT_MINIMAX_API_HOST: &str = "https://api.minimaxi.com";

/// 检索后端选择（设置页可强制，默认自动）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum WebSearchBackendMode {
    #[default]
    Auto,
    Minimax,
    Duckduckgo,
}


impl WebSearchBackendMode {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "minimax" => Self::Minimax,
            "duckduckgo" | "ddg" => Self::Duckduckgo,
            _ => Self::Auto,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Minimax => "minimax",
            Self::Duckduckgo => "duckduckgo",
        }
    }
}

/// 实际完成检索的后端（写入元数据 / 连通性）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebSearchEffectiveBackend {
    Minimax,
    Duckduckgo,
}

impl WebSearchEffectiveBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Minimax => "minimax",
            Self::Duckduckgo => "duckduckgo",
        }
    }
}

#[derive(Debug, Clone)]
pub struct WebSearchPreferences {
    pub backend_mode: WebSearchBackendMode,
    pub minimax_api_host: String,
}

impl Default for WebSearchPreferences {
    fn default() -> Self {
        Self {
            backend_mode: WebSearchBackendMode::Auto,
            minimax_api_host: DEFAULT_MINIMAX_API_HOST.to_string(),
        }
    }
}

/// 从 settings 表加载联网检索偏好。
pub fn load(db: &Database) -> AppResult<WebSearchPreferences> {
    let mut prefs = WebSearchPreferences::default();
    if let Ok(Some(host)) = read_setting(db, MINIMAX_API_HOST_KEY) {
        let trimmed = host.trim();
        if !trimmed.is_empty() {
            prefs.minimax_api_host = trimmed.to_string();
        }
    }
    if let Ok(Some(mode)) = read_setting(db, WEB_SEARCH_BACKEND_KEY) {
        prefs.backend_mode = WebSearchBackendMode::parse(&mode);
    }
    Ok(prefs)
}

fn read_setting(db: &Database, key: &str) -> AppResult<Option<String>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query([key])?;
        if let Some(row) = rows.next()? {
            let v: String = row.get(0)?;
            Ok(Some(v))
        } else {
            Ok(None)
        }
    })
}

/// 保存 MiniMax API Host。
pub fn save_minimax_api_host(db: &Database, host: &str) -> AppResult<()> {
    let trimmed = host.trim();
    if trimmed.is_empty() {
        return Err(crate::error::AppError::msg("MiniMax API Host 不能为空"));
    }
    write_setting(db, MINIMAX_API_HOST_KEY, trimmed)
}

/// 保存检索后端模式。
pub fn save_web_search_backend(db: &Database, mode: WebSearchBackendMode) -> AppResult<()> {
    write_setting(db, WEB_SEARCH_BACKEND_KEY, mode.as_str())
}

fn write_setting(db: &Database, key: &str, value: &str) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_backend_mode() {
        assert_eq!(
            WebSearchBackendMode::parse("minimax"),
            WebSearchBackendMode::Minimax
        );
        assert_eq!(
            WebSearchBackendMode::parse("duckduckgo"),
            WebSearchBackendMode::Duckduckgo
        );
        assert_eq!(WebSearchBackendMode::parse(""), WebSearchBackendMode::Auto);
    }
}
