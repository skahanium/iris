use std::sync::LazyLock;
use std::time::Duration;

use reqwest::ClientBuilder;

use crate::error::AppResult;

const DEFAULT_TIMEOUT_SECS: u64 = 300;
const DEFAULT_READ_TIMEOUT_SECS: u64 = 60;
const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;

/// 创建带有安全 TLS 配置的 HTTP client builder（无证书固定）。
///
/// 调用方可在 `.build()` 前追加自定义配置（如 `.user_agent()`）。
///
/// - 强制 HTTPS（拒绝明文 HTTP）
/// - 使用 rustls TLS 后端（不依赖系统 OpenSSL）
/// - 总超时 300 秒（普通非流式请求整体 deadline，兜底长请求上界）
/// - 连接超时 10 秒，避免 DNS/TCP/TLS 建连无限等待
/// - 读超时 60 秒（每次读操作超时，成功读后重置；用于检测 SSE/流式
///   stalled 连接——某些 provider 在 `[DONE]` 后保持 socket 打开，或
///   中途停止发送，`read_timeout` 能在 60s 无数据时强制断流，而不是
///   靠总超时等待。对非流式请求无副作用）
pub fn https_client_builder() -> ClientBuilder {
    reqwest::Client::builder()
        .use_rustls_tls()
        .https_only(true)
        .connect_timeout(Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
        .read_timeout(Duration::from_secs(DEFAULT_READ_TIMEOUT_SECS))
}

/// 创建专用于流式/SSE 请求的 HTTPS client builder。
///
/// 流式请求不能设置总 300 秒 timeout，否则连续输出的长文会被墙钟误杀；
/// 这里仅保留 per-read `read_timeout` 作为 stall 检测，并由调用方的
/// abort poll 处理用户手动停止。
pub fn https_streaming_client_builder() -> ClientBuilder {
    reqwest::Client::builder()
        .use_rustls_tls()
        .https_only(true)
        .connect_timeout(Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS))
        .read_timeout(Duration::from_secs(DEFAULT_READ_TIMEOUT_SECS))
}

/// 返回全局单例 Client 的克隆，共享 HTTP 连接池 (keep-alive)。
pub fn create_https_client() -> AppResult<reqwest::Client> {
    Ok(GLOBAL_HTTPS_CLIENT.clone())
}

/// 返回全局流式 Client 的克隆，共享 HTTP 连接池 (keep-alive)。
pub fn create_streaming_https_client() -> AppResult<reqwest::Client> {
    Ok(GLOBAL_STREAMING_HTTPS_CLIENT.clone())
}

/// 全局 HTTP Client 单例，首次访问时创建。
/// reqwest::Client 内部使用 Arc 连接池。
static GLOBAL_HTTPS_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    https_client_builder()
        .build()
        .expect("Failed to build global HTTP client")
});

/// 全局流式 HTTP Client 单例，首次访问时创建。
static GLOBAL_STREAMING_HTTPS_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    https_streaming_client_builder()
        .build()
        .expect("Failed to build global streaming HTTP client")
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_client_uses_default_tls_stack() {
        assert!(create_https_client().is_ok());
        assert!(create_streaming_https_client().is_ok());
    }
}
