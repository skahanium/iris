//! 有界 Feed / 发现页获取。//!
//! 阶段 2 Task 2.4/2.5（discovery/sync）将消费这些能力；届时移除本标注。
#![allow(dead_code)]

//!
//! [`FeedHttpClient::fetch`] 逐跳完成：URL 校验 → DNS 解析并整体校验 →
//! 固定连接 → 条件请求 → 有界 streaming；重定向每跳重新解析与重新固定，
//! 最多 5 跳。网络校验通过 [`FeedNetGate`] 注入，生产使用 `safe_https`，
//! 测试注入允许本地服务器的宽松网门。

use std::collections::HashSet;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::header::{IF_MODIFIED_SINCE, IF_NONE_MATCH, LOCATION};
use reqwest::{Client, StatusCode};

use crate::error::{AppError, AppResult};

/// Feed 响应体积上限（5 MiB）。
pub(crate) const FEED_MAX_BYTES: usize = 5 * 1024 * 1024;
/// 发现页响应体积上限（2 MiB）。
pub(crate) const DISCOVERY_MAX_BYTES: usize = 2 * 1024 * 1024;
/// 单跳请求总超时。
pub(crate) const FETCH_TIMEOUT: Duration = Duration::from_secs(20);
/// 重定向最大跳数。
pub(crate) const MAX_REDIRECTS: usize = 5;

/// 只包含 `Iris/<version> RSS Reader`，不含 Vault、设备名或用户 ID。
const USER_AGENT: &str = concat!("Iris/", env!("CARGO_PKG_VERSION"), " RSS Reader");

/// 获取目的：决定体积上限。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FetchPurpose {
    Feed,
    Discovery,
}

impl FetchPurpose {
    fn max_bytes(self) -> usize {
        match self {
            Self::Feed => FEED_MAX_BYTES,
            Self::Discovery => DISCOVERY_MAX_BYTES,
        }
    }
}

/// 有界获取结果。
#[derive(Debug, Clone)]
pub(crate) struct FeedFetchResult {
    pub status: u16,
    pub final_url: String,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub bytes: Vec<u8>,
}

/// 网络校验门：生产实现走 `network::safe_https`，测试注入宽松实现。
pub(crate) trait FeedNetGate: Send + Sync {
    fn validate_url(&self, url: &str) -> AppResult<()>;
    async fn resolve_public_addrs(&self, host: &str) -> AppResult<Vec<IpAddr>>;
    fn build_client(&self, host: &str, port: u16, addrs: &[IpAddr]) -> AppResult<Client>;
}

/// 生产网门：完整 SSRF 校验 + DNS pinning + 20 秒超时。
pub(crate) struct ProdNetGate;

impl FeedNetGate for ProdNetGate {
    fn validate_url(&self, url: &str) -> AppResult<()> {
        crate::network::safe_https::validate_https_url(url)
    }

    async fn resolve_public_addrs(&self, host: &str) -> AppResult<Vec<IpAddr>> {
        crate::network::safe_https::resolve_public_addrs(host).await
    }

    fn build_client(&self, host: &str, port: u16, addrs: &[IpAddr]) -> AppResult<Client> {
        crate::network::safe_https::build_pinned_client_with_timeout(
            host,
            port,
            addrs,
            FETCH_TIMEOUT,
        )
    }
}

/// 无状态有界获取器。
pub(crate) struct FeedHttpClient;

impl FeedHttpClient {
    /// 有界获取：逐跳校验 → 解析 → 固定 → 条件请求 → 有界 streaming。
    ///
    /// 重定向由本层逐跳处理（最多 [`MAX_REDIRECTS`] 跳），每跳重新解析与
    /// 重新固定；`log_id` 仅用于日志中的稳定标识（如 source id），日志
    /// 永不包含 URL、请求头或响应体。
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn fetch<G: FeedNetGate>(
        &self,
        gate: &G,
        url: &str,
        purpose: FetchPurpose,
        etag: Option<&str>,
        last_modified: Option<&str>,
        log_id: Option<&str>,
    ) -> AppResult<FeedFetchResult> {
        let max_bytes = purpose.max_bytes();
        let mut current = url.trim().to_string();
        gate.validate_url(&current)?;
        let mut visited: HashSet<String> = HashSet::new();
        let started = Instant::now();
        let request_etag = etag.map(str::to_string);
        let request_last_modified = last_modified.map(str::to_string);

        for _hop in 0..=MAX_REDIRECTS {
            if !visited.insert(current.clone()) {
                return Err(AppError::msg("feed_redirect_loop"));
            }
            let (host, port) = host_port_of(&current)?;
            let addrs = gate.resolve_public_addrs(&host).await?;
            let client = gate.build_client(&host, port, &addrs)?;

            let mut request = client
                .get(&current)
                .header(reqwest::header::USER_AGENT, USER_AGENT);
            if let Some(value) = &request_etag {
                request = request.header(IF_NONE_MATCH, value);
            }
            if let Some(value) = &request_last_modified {
                request = request.header(IF_MODIFIED_SINCE, value);
            }
            let response = request
                .send()
                .await
                // 稳定码：reqwest 错误串内含 URL，禁止进入错误消息或日志。
                .map_err(|_| AppError::msg("feed_fetch_failed"))?;
            let status = response.status();
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let response_etag = response
                .headers()
                .get(reqwest::header::ETAG)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let response_last_modified = response
                .headers()
                .get(reqwest::header::LAST_MODIFIED)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);

            if status == StatusCode::NOT_MODIFIED {
                Self::log_complete(log_id, status_class(status), 0, started);
                return Ok(FeedFetchResult {
                    status: status.as_u16(),
                    final_url: current,
                    content_type,
                    etag: response_etag,
                    last_modified: response_last_modified,
                    bytes: Vec::new(),
                });
            }

            if status.is_redirection() {
                let location = response
                    .headers()
                    .get(LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| AppError::msg("feed_redirect_missing_location"))?;
                let next = reqwest::Url::parse(&current)
                    .map_err(|_| AppError::msg("feed_url_invalid"))?
                    .join(location)
                    .map_err(|_| AppError::msg("feed_redirect_invalid_target"))?
                    .to_string();
                // 每次重定向都重新校验目标（防跨到私网/降级协议）。
                gate.validate_url(&next)?;
                current = next;
                continue;
            }

            if status == StatusCode::NOT_MODIFIED {
                Self::log_complete(log_id, status_class(status), 0, started);
                return Ok(FeedFetchResult {
                    status: status.as_u16(),
                    final_url: current,
                    content_type,
                    etag: response_etag,
                    last_modified: response_last_modified,
                    bytes: Vec::new(),
                });
            }
            if !status.is_success() {
                tracing::warn!(
                    log_id = log_id.unwrap_or("feed"),
                    status_class = status_class(status),
                    "feed_fetch_error"
                );
                return Err(AppError::msg(format!(
                    "feed_http_error_{}",
                    status.as_u16()
                )));
            }

            // Content-Length 预拒绝：声明即超限则不读取响应体。
            if let Some(length) = response.content_length() {
                if length as usize > max_bytes {
                    return Err(AppError::msg("feed_response_too_large"));
                }
            }
            // 有界 streaming：流中累计超限立即中止。
            let mut bytes: Vec<u8> = Vec::new();
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                // 稳定码：流错误串可能含 URL，禁止进入错误消息。
                let chunk = chunk.map_err(|_| AppError::msg("feed_stream_error"))?;
                if bytes.len() + chunk.len() > max_bytes {
                    return Err(AppError::msg("feed_response_too_large"));
                }
                bytes.extend_from_slice(&chunk);
            }

            Self::log_complete(log_id, status_class(status), bytes.len(), started);
            return Ok(FeedFetchResult {
                status: status.as_u16(),
                final_url: current,
                content_type,
                etag: response_etag,
                last_modified: response_last_modified,
                bytes,
            });
        }
        Err(AppError::msg("feed_too_many_redirects"))
    }

    fn log_complete(
        log_id: Option<&str>,
        status_class: &'static str,
        bytes: usize,
        started: Instant,
    ) {
        tracing::info!(
            log_id = log_id.unwrap_or("feed"),
            status_class,
            bytes,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "feed_fetch_complete"
        );
    }
}

fn host_port_of(url: &str) -> AppResult<(String, u16)> {
    let parsed = reqwest::Url::parse(url).map_err(|_| AppError::msg("feed_url_invalid"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::msg("feed_url_invalid"))?
        .to_string();
    let port = parsed.port().unwrap_or_else(|| match parsed.scheme() {
        "https" => 443,
        "http" => 80,
        _ => 443,
    });
    Ok((host, port))
}

fn status_class(status: StatusCode) -> &'static str {
    if status.is_success() {
        "success"
    } else if status == StatusCode::NOT_MODIFIED {
        "not_modified"
    } else if status.is_redirection() {
        "redirect"
    } else {
        "error"
    }
}
