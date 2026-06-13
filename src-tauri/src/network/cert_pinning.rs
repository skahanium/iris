use std::sync::LazyLock;
use std::time::Duration;

use reqwest::ClientBuilder;

use crate::error::AppResult;

const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// 创建带有安全 TLS 配置的 HTTP client builder（无证书固定）。
///
/// 调用方可在 `.build()` 前追加自定义配置（如 `.user_agent()`）。
///
/// - 强制 HTTPS（拒绝明文 HTTP）
/// - 使用 rustls TLS 后端（不依赖系统 OpenSSL）
/// - 默认 60 秒超时
pub fn https_client_builder() -> ClientBuilder {
    reqwest::Client::builder()
        .use_rustls_tls()
        .https_only(true)
        .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
}

/// 返回全局单例 Client 的克隆，共享 HTTP 连接池 (keep-alive)。
pub fn create_https_client() -> AppResult<reqwest::Client> {
    Ok(GLOBAL_HTTPS_CLIENT.clone())
}

/// 全局 HTTP Client 单例，首次访问时创建。
/// reqwest::Client 内部使用 Arc 连接池。
static GLOBAL_HTTPS_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    https_client_builder()
        .build()
        .expect("Failed to build global HTTP client")
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_client_uses_default_tls_stack() {
        assert!(create_https_client().is_ok());
    }
}
