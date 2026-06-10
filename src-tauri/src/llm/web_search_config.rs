//! 联网检索偏好（SQLite settings，与对话路由分离）。

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::storage::db::Database;

pub const MINIMAX_API_HOST_KEY: &str = "minimax_api_host";
pub const MINIMAX_SEARCH_MODEL_KEY: &str = "minimax_search_model";
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
    /// 联网检索请求体中的 `model` 字段；空字符串表示不传（使用 Coding Plan 服务端默认）。
    pub minimax_search_model: String,
}

impl Default for WebSearchPreferences {
    fn default() -> Self {
        Self {
            backend_mode: WebSearchBackendMode::Auto,
            minimax_api_host: DEFAULT_MINIMAX_API_HOST.to_string(),
            minimax_search_model: String::new(),
        }
    }
}

/// 从 settings 表加载联网检索偏好。
pub fn load(db: &Database) -> AppResult<WebSearchPreferences> {
    let mut prefs = WebSearchPreferences::default();
    if let Ok(Some(host)) = read_setting(db, MINIMAX_API_HOST_KEY) {
        if let Ok(normalized) = normalize_minimax_api_host(&host) {
            prefs.minimax_api_host = normalized;
        }
    }
    if let Ok(Some(mode)) = read_setting(db, WEB_SEARCH_BACKEND_KEY) {
        prefs.backend_mode = WebSearchBackendMode::parse(&mode);
    }
    if let Ok(Some(model)) = read_setting(db, MINIMAX_SEARCH_MODEL_KEY) {
        prefs.minimax_search_model = model.trim().to_string();
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
    let normalized = normalize_minimax_api_host(host)?;
    write_setting(db, MINIMAX_API_HOST_KEY, &normalized)
}

fn normalize_minimax_api_host(host: &str) -> AppResult<String> {
    let trimmed = host.trim();
    if trimmed.is_empty() {
        return Err(crate::error::AppError::msg(
            "MiniMax API Host cannot be empty",
        ));
    }
    if trimmed.contains('\0') {
        return Err(crate::error::AppError::msg("Invalid MiniMax API Host"));
    }
    let mut url = reqwest::Url::parse(trimmed)
        .map_err(|_| crate::error::AppError::msg("Invalid MiniMax API Host"))?;
    if url.scheme() != "https" {
        return Err(crate::error::AppError::msg(
            "MiniMax API Host must use HTTPS",
        ));
    }
    if url.host_str().is_none() {
        return Err(crate::error::AppError::msg(
            "MiniMax API Host must include a host",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(crate::error::AppError::msg(
            "MiniMax API Host must not include credentials",
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(crate::error::AppError::msg(
            "MiniMax API Host must not include query or fragment",
        ));
    }
    if url.path() != "/" {
        return Err(crate::error::AppError::msg(
            "MiniMax API Host must not include a path",
        ));
    }
    url.set_path("");
    Ok(url.as_str().trim_end_matches('/').to_string())
}

/// 保存检索后端模式。
pub fn save_web_search_backend(db: &Database, mode: WebSearchBackendMode) -> AppResult<()> {
    write_setting(db, WEB_SEARCH_BACKEND_KEY, mode.as_str())
}

/// 保存 MiniMax 联网检索模型名（空字符串表示清除自定义，回退服务端默认）。
pub fn save_minimax_search_model(db: &Database, model: &str) -> AppResult<()> {
    write_setting(db, MINIMAX_SEARCH_MODEL_KEY, model.trim())
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
    fn load_persists_minimax_search_model() {
        let db = crate::storage::db::Database::open_in_memory().unwrap();
        save_minimax_search_model(&db, "MiniMax-M2.5").unwrap();
        let prefs = load(&db).unwrap();
        assert_eq!(prefs.minimax_search_model, "MiniMax-M2.5");
    }

    #[test]
    fn save_minimax_api_host_requires_clean_https_origin() {
        let db = crate::storage::db::Database::open_in_memory().unwrap();

        assert!(save_minimax_api_host(&db, "http://api.minimaxi.com").is_err());
        assert!(save_minimax_api_host(&db, "https://user@api.minimaxi.com").is_err());
        assert!(save_minimax_api_host(&db, "https://api.minimaxi.com?x=1").is_err());
        assert!(save_minimax_api_host(&db, "https://api.minimaxi.com#frag").is_err());

        save_minimax_api_host(&db, " https://api.minimaxi.com/ ").unwrap();
        let prefs = load(&db).unwrap();
        assert_eq!(prefs.minimax_api_host, "https://api.minimaxi.com");
    }

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
