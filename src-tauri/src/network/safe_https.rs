//! 公共 HTTPS 出站校验与 DNS pinning（SSRF 防御）。
//!
//! 统一承担三类能力，供网页抓取与订阅获取共用，禁止复制第二套地址判断：
//! - `validate_https_url`：初始与每一跳重定向的 URL 级完整校验；
//! - `resolve_public_addrs` / `validate_resolved_addrs`：DNS 解析后全部地址
//!   必须为公网，任何一条解析地址被拒绝都拒绝该主机；
//! - `build_pinned_client_with_timeout`：把本次连接固定到已校验地址（防 DNS rebinding）。

use std::net::{IpAddr, SocketAddr};

use reqwest::Client;

use crate::error::{AppError, AppResult};

/// 单个 HTTPS 响应允许的响应头总字节预算（名称 + 值）。
pub(crate) const MAX_RESPONSE_HEADER_BYTES: usize = 32 * 1024;

/// 安全 HTTPS 出站网门：生产与测试共用同一逐跳执行契约。
pub(crate) trait SafeHttpsGate: Send + Sync {
    fn validate_url(&self, url: &str) -> AppResult<()>;
    async fn resolve_public_addrs(&self, host: &str) -> AppResult<Vec<IpAddr>>;
    fn build_client(
        &self,
        host: &str,
        port: u16,
        addrs: &[IpAddr],
        timeout: std::time::Duration,
    ) -> AppResult<Client>;

    fn total_timeout(&self, requested: std::time::Duration) -> std::time::Duration {
        requested
    }
}

/// 使用公共 URL 校验、DNS 全地址拒绝与连接 pinning 的生产网门。
pub(crate) struct ProdSafeHttpsGate;

impl SafeHttpsGate for ProdSafeHttpsGate {
    fn validate_url(&self, url: &str) -> AppResult<()> {
        validate_https_url(url)
    }

    async fn resolve_public_addrs(&self, host: &str) -> AppResult<Vec<IpAddr>> {
        resolve_public_addrs(host).await
    }

    fn build_client(
        &self,
        host: &str,
        port: u16,
        addrs: &[IpAddr],
        timeout: std::time::Duration,
    ) -> AppResult<Client> {
        build_pinned_client_with_timeout(host, port, addrs, timeout)
    }
}

/// URL 级完整校验：仅 HTTPS、无 userinfo、拒绝 localhost / IPv4 / IPv6 私网 /
/// link-local / metadata / 保留段与私网域名提示（DNS rebinding 提示）。
pub fn validate_https_url(url: &str) -> AppResult<()> {
    crate::security::ipc_policy::validate_https_url(url)
        .map_err(|_| AppError::msg("https_url_invalid"))?;
    let host = host_of(url).ok_or_else(|| AppError::msg("https_url_invalid"))?;
    let host_lower = host.to_lowercase();
    if host_lower == "localhost" || host_lower.ends_with(".localhost") {
        return Err(AppError::msg("https_url_private"));
    }
    if host_lower == "0.0.0.0" {
        return Err(AppError::msg("https_url_private"));
    }
    if let Ok(ip) = host_lower.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err(AppError::msg("https_url_private"));
        }
        return Err(AppError::msg("https_url_ip_literal"));
    }
    if is_private_host_hint(&host_lower) {
        return Err(AppError::msg("https_url_private"));
    }
    Ok(())
}

/// 提取 URL 主机名（剥除 IPv6 括号）；userinfo 或不可解析返回 `None`。
pub fn host_of(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }
    let host = parsed.host_str()?;
    Some(
        host.strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .unwrap_or(host)
            .to_owned(),
    )
}

/// 纯决策：全部地址必须为公网，任一被拒则整体拒绝；空解析报错。
pub fn validate_resolved_addrs(addrs: &[IpAddr]) -> AppResult<Vec<IpAddr>> {
    if addrs.is_empty() {
        return Err(AppError::msg("https_dns_empty"));
    }
    if addrs.iter().any(|ip| is_blocked_ip(*ip)) {
        return Err(AppError::msg("https_dns_private"));
    }
    Ok(addrs.to_vec())
}

/// 解析主机全部地址并整体校验（任何一条私网地址都拒绝该主机）。
pub async fn resolve_public_addrs(host: &str) -> AppResult<Vec<IpAddr>> {
    let mut addrs: Vec<IpAddr> = Vec::new();
    for socket in tokio::net::lookup_host((host, 0))
        .await
        .map_err(|_| AppError::msg("https_dns_failed"))?
    {
        let ip = socket.ip();
        if !addrs.contains(&ip) {
            addrs.push(ip);
        }
    }
    validate_resolved_addrs(&addrs)
}

/// 带自定义总超时的 pinning 变体（订阅获取要求 20 秒总预算）。
pub(crate) fn build_pinned_client_with_timeout(
    host: &str,
    port: u16,
    addrs: &[IpAddr],
    timeout: std::time::Duration,
) -> AppResult<Client> {
    // SSRF 防线要求连接目标就是本地已校验并固定的地址。HTTP(S) 代理会把
    // CONNECT 目标交给代理端重新解析，从而绕过 `resolve`；安全抓取因此
    // 明确禁用所有代理，只保留 rustls、HTTPS-only 与直连 pinning。
    let builder = reqwest::Client::builder()
        .use_rustls_tls()
        .https_only(true)
        .no_proxy()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(timeout)
        .read_timeout(timeout);
    pinned_builder(builder, host, port, addrs)
        .build()
        .map_err(|_| AppError::msg("https_client_build_failed"))
}

fn pinned_builder(
    builder: reqwest::ClientBuilder,
    host: &str,
    port: u16,
    addrs: &[IpAddr],
) -> reqwest::ClientBuilder {
    let mut builder = builder.redirect(reqwest::redirect::Policy::none());
    for addr in addrs {
        builder = builder.resolve(host, SocketAddr::new(*addr, port));
    }
    builder
}

/// 应用层响应头总字节预算。
///
/// reqwest 当前只暴露 HTTP/2 的 header-list builder 上限；HTTP/1 仍由
/// hyper 的固定 header-count 上限兜底，因此在解析后统一执行同一字节预算。
pub(crate) fn validate_response_headers(headers: &reqwest::header::HeaderMap) -> AppResult<()> {
    let bytes = headers.iter().try_fold(0usize, |total, (name, value)| {
        total
            .checked_add(name.as_str().len())
            .and_then(|size| size.checked_add(value.as_bytes().len()))
            .and_then(|size| size.checked_add(4))
            .ok_or_else(|| AppError::msg("https_response_headers_too_large"))
    })?;
    if bytes > MAX_RESPONSE_HEADER_BYTES {
        return Err(AppError::msg("https_response_headers_too_large"));
    }
    Ok(())
}

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || (v4.octets()[0] == 169 && v4.octets()[1] == 254)
                // RFC 6598 Carrier-grade NAT
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64)
                // RFC 2544 benchmarking
                || (v4.octets()[0] == 198 && v4.octets()[1] >= 18 && v4.octets()[1] <= 19)
        }
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified() || is_ipv6_private(v6),
    }
}

fn is_ipv6_private(v6: std::net::Ipv6Addr) -> bool {
    let s = v6.segments();
    // fc00::/7 — Unique Local Address
    (s[0] & 0xFE00) == 0xFC00
    // fe80::/10 — Link-local
    || (s[0] & 0xFFC0) == 0xFE80
    // ::ffff:0:0/96 — IPv4-mapped IPv6
    || (s[0] == 0 && s[1] == 0 && s[2] == 0 && s[3] == 0 && s[4] == 0 && s[5] == 0xFFFF)
    // 64:ff9b::/96 — IPv4/IPv6 translation
    || (s[0] == 0x0064 && s[1] == 0xFF9B)
    // ::1 — loopback (defense-in-depth)
    || v6.is_loopback()
}

fn is_private_host_hint(host: &str) -> bool {
    // Try parsing as IP address first
    if let Ok(ip) = host.parse::<IpAddr>() {
        return is_blocked_ip(ip);
    }

    // DNS rebinding detection: domain names containing private IP octets
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() >= 4 {
        if let (Ok(a), Ok(b)) = (parts[0].parse::<u8>(), parts[1].parse::<u8>()) {
            // 10.x.x.x, 127.x.x.x, 192.168.x.x
            if a == 10
                || a == 127
                || (a == 192 && b == 168)
                // 172.16-31.x.x
                || (a == 172 && (16..=31).contains(&b))
                // 169.254.x.x
                || (a == 169 && b == 254)
            {
                return true;
            }
        }
    }

    // Common private/intranet domain suffixes
    host.ends_with(".local")
        || host.ends_with(".internal")
        || host.ends_with(".localhost")
        || host.ends_with(".lan")
        || host == "localhost"
}
