use reqwest::Client;
use reqwest::ClientBuilder;
use std::time::Duration;

use crate::error::{AppError, AppResult};

const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// 创建带有安全 TLS 配置的 HTTP client builder。
///
/// 调用方可在 `.build()` 前追加自定义配置（如 `.user_agent()`）。
///
/// - 强制 HTTPS（拒绝明文 HTTP）
/// - 使用 rustls TLS 后端（不依赖系统 OpenSSL）
/// - 默认 60 秒超时
pub fn pinned_client_builder() -> ClientBuilder {
    Client::builder()
        .use_rustls_tls()
        .https_only(true)
        .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
}

/// 创建带有安全 TLS 配置的 HTTP client。
///
/// 注意：当前不绑定特定证书指纹。证书固定可作为额外安全层，
/// 但需要定期更新指纹，维护成本较高。
pub fn create_pinned_client() -> AppResult<Client> {
    pinned_client_builder()
        .build()
        .map_err(|e| AppError::msg(format!("Failed to build HTTP client: {e}")))
}
