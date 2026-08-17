//! 有界 Feed / 发现页获取。//!

//!
//! [`FeedHttpClient::fetch`] 逐跳完成：URL 校验 → DNS 解析并整体校验 →
//! 固定连接 → 条件请求 → 有界 streaming；重定向每跳重新解析与重新固定，
//! 最多 5 跳。网络校验通过 [`FeedNetGate`] 注入，生产使用 `safe_https`，
//! 测试注入允许本地服务器的宽松网门。

use std::collections::HashSet;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::header::{IF_MODIFIED_SINCE, IF_NONE_MATCH, LOCATION};
use reqwest::StatusCode;

use crate::error::{AppError, AppResult};

pub(crate) use crate::network::safe_https::ProdSafeHttpsGate as ProdNetGate;
pub(crate) use crate::network::safe_https::SafeHttpsGate as FeedNetGate;

/// Feed 响应体积上限（5 MiB）。
pub(crate) const FEED_MAX_BYTES: usize = 5 * 1024 * 1024;
/// 发现页响应体积上限（2 MiB）。
pub(crate) const DISCOVERY_MAX_BYTES: usize = 2 * 1024 * 1024;
/// 网页正文响应体积上限（1 MiB）。正文提取会建立 DOM，较 Feed 更严格以
/// 控制峰值内存；超限时安全降级为 RSS 摘要。
pub(crate) const ARTICLE_MAX_BYTES: usize = 1024 * 1024;
/// 远程图片导入上限（10 MiB）。
pub(crate) const IMAGE_MAX_BYTES: usize = 10 * 1024 * 1024;
/// 一条重定向链共享的总超时。
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
    Article,
    Image,
}

impl FetchPurpose {
    fn max_bytes(self) -> usize {
        match self {
            Self::Feed => FEED_MAX_BYTES,
            Self::Discovery => DISCOVERY_MAX_BYTES,
            Self::Article => ARTICLE_MAX_BYTES,
            Self::Image => IMAGE_MAX_BYTES,
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
        tokio::time::timeout(
            gate.total_timeout(FETCH_TIMEOUT),
            self.fetch_within_deadline(gate, url, purpose, etag, last_modified, log_id),
        )
        .await
        .map_err(|_| AppError::msg("feed_fetch_timeout"))?
    }

    async fn fetch_within_deadline<G: FeedNetGate>(
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
        gate.validate_url(&current)
            .map_err(|_| AppError::msg("feed_url_rejected"))?;
        let mut visited: HashSet<String> = HashSet::new();
        let initial_authority = reqwest::Url::parse(&current).ok().and_then(|parsed| {
            parsed
                .host_str()
                .map(|host| (host.to_string(), parsed.port_or_known_default()))
        });
        let started = Instant::now();
        let request_etag = etag.map(str::to_string);
        let request_last_modified = last_modified.map(str::to_string);

        for _hop in 0..=MAX_REDIRECTS {
            if !visited.insert(current.clone()) {
                return Err(AppError::msg("feed_redirect_loop"));
            }
            let (host, port) = host_port_of(&current)?;
            let addrs = gate
                .resolve_public_addrs(&host)
                .await
                .map_err(|_| AppError::msg("feed_dns_failed"))?;
            if gate.uses_fixed_proxy_transport() {
                let mut conditional = Vec::new();
                let same_authority = reqwest::Url::parse(&current).ok().and_then(|parsed| {
                    parsed
                        .host_str()
                        .map(|host| (host.to_string(), parsed.port_or_known_default()))
                }) == initial_authority;
                if same_authority {
                    if let Some(value) = &request_etag {
                        conditional.push((IF_NONE_MATCH.as_str(), value.as_str()));
                    }
                    if let Some(value) = &request_last_modified {
                        conditional.push((IF_MODIFIED_SINCE.as_str(), value.as_str()));
                    }
                }
                let response = crate::network::safe_https::fixed_https_get(
                    &current,
                    &addrs,
                    USER_AGENT,
                    &conditional,
                    max_bytes,
                )
                .await
                .map_err(map_fixed_transport_error)?;
                let status = response.status;
                let content_type = response
                    .headers
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string);
                let response_etag = response
                    .headers
                    .get(reqwest::header::ETAG)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string);
                let response_last_modified = response
                    .headers
                    .get(reqwest::header::LAST_MODIFIED)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string);
                if status == 304 {
                    return Ok(FeedFetchResult {
                        status,
                        final_url: current,
                        content_type,
                        etag: response_etag,
                        last_modified: response_last_modified,
                        bytes: Vec::new(),
                    });
                }
                if (300..400).contains(&status) {
                    let location = response
                        .headers
                        .get(LOCATION)
                        .and_then(|v| v.to_str().ok())
                        .ok_or_else(|| AppError::msg("feed_redirect_missing_location"))?;
                    let next = reqwest::Url::parse(&current)
                        .map_err(|_| AppError::msg("feed_url_invalid"))?
                        .join(location)
                        .map_err(|_| AppError::msg("feed_redirect_invalid_target"))?
                        .to_string();
                    gate.validate_url(&next)
                        .map_err(|_| AppError::msg("feed_url_rejected"))?;
                    current = next;
                    continue;
                }
                if !(200..300).contains(&status) {
                    return Err(AppError::msg(format!("feed_http_error_{status}")));
                }
                return Ok(FeedFetchResult {
                    status,
                    final_url: current,
                    content_type,
                    etag: response_etag,
                    last_modified: response_last_modified,
                    bytes: response.bytes,
                });
            }
            let client = gate
                .build_client(&host, port, &addrs, gate.total_timeout(FETCH_TIMEOUT))
                .map_err(|_| AppError::msg("feed_client_build_failed"))?;

            let mut request = client
                .get(&current)
                .header(reqwest::header::USER_AGENT, USER_AGENT);
            let same_authority = reqwest::Url::parse(&current).ok().and_then(|parsed| {
                parsed
                    .host_str()
                    .map(|host| (host.to_string(), parsed.port_or_known_default()))
            }) == initial_authority;
            // ETag/Last-Modified 属于原始订阅源的缓存验证器。跨 authority
            // 重定向时绝不能把它们发送给新的站点。
            if same_authority {
                if let Some(value) = &request_etag {
                    request = request.header(IF_NONE_MATCH, value);
                }
                if let Some(value) = &request_last_modified {
                    request = request.header(IF_MODIFIED_SINCE, value);
                }
            }
            let response = request
                .send()
                .await
                // 稳定码：reqwest 错误串内含 URL，禁止进入错误消息或日志。
                .map_err(|error| {
                    if error.is_timeout() {
                        AppError::msg("feed_fetch_timeout")
                    } else {
                        AppError::msg("feed_fetch_failed")
                    }
                })?;
            crate::network::safe_https::validate_response_headers(response.headers())
                .map_err(|_| AppError::msg("feed_response_headers_too_large"))?;
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
                gate.validate_url(&next)
                    .map_err(|_| AppError::msg("feed_url_rejected"))?;
                current = next;
                continue;
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

fn map_fixed_transport_error(error: AppError) -> AppError {
    match error.to_string().as_str() {
        "https_proxy_unreachable" => AppError::msg("feed_proxy_unreachable"),
        "https_proxy_unsupported" => AppError::msg("feed_proxy_unsupported"),
        "https_proxy_auth_unsupported" => AppError::msg("feed_proxy_auth_unsupported"),
        "https_proxy_connect_failed" => AppError::msg("feed_proxy_connect_failed"),
        "https_response_headers_too_large" => AppError::msg("feed_response_headers_too_large"),
        "https_response_too_large" => AppError::msg("feed_response_too_large"),
        "https_stream_failed" => AppError::msg("feed_stream_error"),
        "https_tls_failed" => AppError::msg("feed_fetch_failed"),
        _ => AppError::msg("feed_fetch_failed"),
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
