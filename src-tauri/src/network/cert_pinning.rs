use std::sync::{RwLock, RwLockReadGuard};
use std::time::Duration;

use reqwest::{Client, ClientBuilder};

use crate::error::{AppError, AppResult};
use crate::network::proxy_policy::{apply_proxy_policy, follow_system_proxy};

const DEFAULT_TIMEOUT_SECS: u64 = 300;
const DEFAULT_READ_TIMEOUT_SECS: u64 = 60;
const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;

struct CachedHttpsClients {
    follow: bool,
    https: Client,
    streaming: Client,
}

static CACHED_CLIENTS: RwLock<Option<CachedHttpsClients>> = RwLock::new(None);

fn base_https_client_builder() -> ClientBuilder {
    reqwest::Client::builder()
        .use_rustls_tls()
        .https_only(true)
        .connect_timeout(Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
        .read_timeout(Duration::from_secs(DEFAULT_READ_TIMEOUT_SECS))
}

fn base_https_streaming_client_builder() -> ClientBuilder {
    reqwest::Client::builder()
        .use_rustls_tls()
        .https_only(true)
        .connect_timeout(Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS))
        .read_timeout(Duration::from_secs(DEFAULT_READ_TIMEOUT_SECS))
}

/// 创建带有安全 TLS 配置的 HTTP client builder（无证书固定）。
///
/// 调用方可在 `.build()` 前追加自定义配置（如 `.user_agent()`）。
/// 已按当前「使用系统代理」偏好应用 [`apply_proxy_policy`]。
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
    apply_proxy_policy(base_https_client_builder())
}

/// 创建专用于流式/SSE 请求的 HTTPS client builder。
///
/// 流式请求不能设置总 300 秒 timeout，否则连续输出的长文会被墙钟误杀；
/// 这里仅保留 per-read `read_timeout` 作为 stall 检测，并由调用方的
/// abort poll 处理用户手动停止。
pub fn https_streaming_client_builder() -> ClientBuilder {
    apply_proxy_policy(base_https_streaming_client_builder())
}

/// Drop cached global clients so the next create_* rebuilds with the current proxy policy.
pub fn invalidate_https_clients() {
    if let Ok(mut guard) = CACHED_CLIENTS.write() {
        *guard = None;
    }
}

fn build_cached_clients(follow: bool) -> AppResult<CachedHttpsClients> {
    let https = https_client_builder()
        .build()
        .map_err(|e| AppError::msg(format!("Failed to build global HTTP client: {e}")))?;
    let streaming = https_streaming_client_builder()
        .build()
        .map_err(|e| AppError::msg(format!("Failed to build global streaming HTTP client: {e}")))?;
    Ok(CachedHttpsClients {
        follow,
        https,
        streaming,
    })
}

fn with_cached_clients<T>(f: impl FnOnce(&CachedHttpsClients) -> T) -> AppResult<T> {
    let follow = follow_system_proxy();
    {
        let guard = read_clients()?;
        if let Some(cached) = guard.as_ref() {
            if cached.follow == follow {
                return Ok(f(cached));
            }
        }
    }

    let built = build_cached_clients(follow)?;
    let mut guard = CACHED_CLIENTS
        .write()
        .map_err(|_| AppError::msg("HTTPS client cache lock poisoned"))?;
    if let Some(cached) = guard.as_ref() {
        if cached.follow == follow {
            return Ok(f(cached));
        }
    }
    *guard = Some(built);
    Ok(f(guard.as_ref().expect("cached clients just inserted")))
}

fn read_clients() -> AppResult<RwLockReadGuard<'static, Option<CachedHttpsClients>>> {
    CACHED_CLIENTS
        .read()
        .map_err(|_| AppError::msg("HTTPS client cache lock poisoned"))
}

/// 返回与当前代理策略匹配的全局 Client 克隆，共享 HTTP 连接池 (keep-alive)。
pub fn create_https_client() -> AppResult<reqwest::Client> {
    with_cached_clients(|cached| cached.https.clone())
}

/// 返回与当前代理策略匹配的全局流式 Client 克隆，共享 HTTP 连接池 (keep-alive)。
pub fn create_streaming_https_client() -> AppResult<reqwest::Client> {
    with_cached_clients(|cached| cached.streaming.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::proxy_policy::store_follow_system_proxy;

    #[test]
    fn https_client_uses_default_tls_stack() {
        let previous = follow_system_proxy();
        store_follow_system_proxy(true);
        invalidate_https_clients();
        assert!(create_https_client().is_ok());
        assert!(create_streaming_https_client().is_ok());
        store_follow_system_proxy(previous);
        invalidate_https_clients();
    }

    #[test]
    fn toggling_proxy_policy_rebuilds_cached_clients() {
        let previous = follow_system_proxy();
        store_follow_system_proxy(true);
        invalidate_https_clients();
        assert!(create_https_client().is_ok());
        store_follow_system_proxy(false);
        invalidate_https_clients();
        assert!(create_https_client().is_ok());
        store_follow_system_proxy(true);
        invalidate_https_clients();
        assert!(create_https_client().is_ok());
        store_follow_system_proxy(previous);
        invalidate_https_clients();
    }
}
