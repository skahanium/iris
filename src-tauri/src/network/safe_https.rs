//! 公共 HTTPS 出站校验与 DNS pinning（SSRF 防御）。
//!
//! 统一承担三类能力，供网页抓取与订阅获取共用，禁止复制第二套地址判断：
//! - `validate_https_url`：初始与每一跳重定向的 URL 级完整校验；
//! - `resolve_public_addrs` / `validate_resolved_addrs`：DNS 解析后全部地址
//!   必须为公网，任何一条解析地址被拒绝都拒绝该主机；
//! - `build_pinned_client_with_timeout`：把本次连接固定到已校验地址（防 DNS rebinding）。

use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use bytes::Bytes;
use http::header::{HOST, USER_AGENT};
use http_body_util::{BodyExt, Empty};
use hyper::body::Body;
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use rustls::pki_types::ServerName;
use rustls_platform_verifier::ConfigVerifierExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use reqwest::Client;

use crate::error::{AppError, AppResult};

/// 单个 HTTPS 响应允许的响应头总字节预算（名称 + 值）。
pub(crate) const MAX_RESPONSE_HEADER_BYTES: usize = 32 * 1024;

/// 安全 HTTPS 出站网门：生产与测试共用同一逐跳执行契约。
pub(crate) trait SafeHttpsGate: Send + Sync {
    fn validate_url(&self, url: &str) -> AppResult<()>;
    fn resolve_public_addrs(
        &self,
        host: &str,
    ) -> impl Future<Output = AppResult<Vec<IpAddr>>> + Send;
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

    /// 生产网门走固定目标隧道；测试网门保留可注入的 reqwest client。
    fn uses_fixed_proxy_transport(&self) -> bool {
        false
    }
}

/// 使用公共 URL 校验、DNS 全地址拒绝与连接 pinning 的生产网门。
pub(crate) struct ProdSafeHttpsGate;

impl SafeHttpsGate for ProdSafeHttpsGate {
    fn validate_url(&self, url: &str) -> AppResult<()> {
        validate_https_url(url)
    }

    fn resolve_public_addrs(
        &self,
        host: &str,
    ) -> impl Future<Output = AppResult<Vec<IpAddr>>> + Send {
        let host = host.to_string();
        async move { resolve_public_addrs(&host).await }
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

    fn uses_fixed_proxy_transport(&self) -> bool {
        true
    }
}

/// 固定地址 HTTPS GET 的已缓冲响应。只供安全抓取使用，不记录目标或代理。
#[derive(Debug, Clone)]
pub(crate) struct FixedHttpsResponse {
    pub status: u16,
    pub headers: reqwest::header::HeaderMap,
    pub bytes: Vec<u8>,
}

/// 经直连、HTTP CONNECT 或 SOCKS5 建立已固定目标的 HTTPS 请求。
pub(crate) async fn fixed_https_get(
    url: &str,
    addrs: &[IpAddr],
    user_agent: &str,
    conditional: &[(&str, &str)],
    max_bytes: usize,
) -> AppResult<FixedHttpsResponse> {
    let parsed = reqwest::Url::parse(url).map_err(|_| AppError::msg("https_url_invalid"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::msg("https_url_invalid"))?;
    let port = parsed.port_or_known_default().unwrap_or(443);
    let target = *addrs
        .first()
        .ok_or_else(|| AppError::msg("https_dns_empty"))?;
    // 连接只会固定到第一个已校验地址；IP/CIDR 型 NO_PROXY 必须针对这一
    // 实际连接目标判断，不能因同一 DNS 响应中的其他地址误走直连。
    let stream =
        match crate::network::system_proxy::snapshot_for(&parsed, std::slice::from_ref(&target)) {
            crate::network::system_proxy::SystemProxySnapshot::Direct => {
                TcpStream::connect(SocketAddr::new(target, port))
                    .await
                    .map_err(|_| AppError::msg("https_connect_failed"))?
            }
            crate::network::system_proxy::SystemProxySnapshot::Unsupported(code) => {
                return Err(AppError::msg(code))
            }
            crate::network::system_proxy::SystemProxySnapshot::Http(proxy) => {
                let stream = connect_proxy(&proxy).await?;
                http_connect(stream, target, port).await?
            }
            crate::network::system_proxy::SystemProxySnapshot::Socks5(proxy) => {
                let stream = connect_proxy(&proxy).await?;
                socks5_connect(stream, target, port).await?
            }
        };
    let config = Arc::new(
        rustls::ClientConfig::with_platform_verifier()
            .map_err(|_| AppError::msg("https_tls_config_failed"))?,
    );
    let server_name =
        ServerName::try_from(host.to_string()).map_err(|_| AppError::msg("https_url_invalid"))?;
    let tls = TlsConnector::from(config)
        .connect(server_name, stream)
        .await
        .map_err(|_| AppError::msg("https_tls_failed"))?;
    let mut builder = http1::Builder::new();
    builder
        .max_headers(100)
        .max_buf_size(MAX_RESPONSE_HEADER_BYTES);
    let (mut sender, connection) = builder
        .handshake(TokioIo::new(tls))
        .await
        .map_err(|_| AppError::msg("https_request_failed"))?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let path = match parsed.query() {
        Some(query) => format!("{}?{query}", parsed.path()),
        None => parsed.path().to_string(),
    };
    let authority = match parsed.port() {
        Some(p) => format!("{host}:{p}"),
        None => host.to_string(),
    };
    let mut request = http::Request::builder()
        .method("GET")
        .uri(path)
        .header(HOST, authority)
        .header(USER_AGENT, user_agent);
    for (name, value) in conditional {
        request = request.header(*name, *value);
    }
    let response = sender
        .send_request(
            request
                .body(Empty::<Bytes>::new())
                .map_err(|_| AppError::msg("https_request_failed"))?,
        )
        .await
        .map_err(|_| AppError::msg("https_request_failed"))?;
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    validate_response_headers(&headers)?;
    if response
        .body()
        .size_hint()
        .upper()
        .is_some_and(|n| n as usize > max_bytes)
    {
        return Err(AppError::msg("https_response_too_large"));
    }
    let mut body = response.into_body();
    let mut bytes = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|_| AppError::msg("https_stream_failed"))?;
        if let Some(data) = frame.data_ref() {
            if bytes.len().saturating_add(data.len()) > max_bytes {
                return Err(AppError::msg("https_response_too_large"));
            }
            bytes.extend_from_slice(data);
        }
    }
    Ok(FixedHttpsResponse {
        status,
        headers,
        bytes,
    })
}

async fn connect_proxy(proxy: &reqwest::Url) -> AppResult<TcpStream> {
    let host = proxy
        .host_str()
        .ok_or_else(|| AppError::msg("https_proxy_unsupported"))?;
    let port = proxy.port_or_known_default().unwrap_or(8080);
    TcpStream::connect((host, port))
        .await
        .map_err(|_| AppError::msg("https_proxy_unreachable"))
}

async fn http_connect(mut stream: TcpStream, target: IpAddr, port: u16) -> AppResult<TcpStream> {
    // RFC 3986 authority 对 IPv6 literal 必须加方括号，否则 `::` 会被代理
    // 误解为 host:port 分隔符。这里始终写入本地已校验 IP，而非原始域名。
    let authority = match target {
        IpAddr::V4(ip) => format!("{ip}:{port}"),
        IpAddr::V6(ip) => format!("[{ip}]:{port}"),
    };
    stream
        .write_all(format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n\r\n").as_bytes())
        .await
        .map_err(|_| AppError::msg("https_proxy_connect_failed"))?;
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 512];
    while !buffer.windows(4).any(|v| v == b"\r\n\r\n") {
        let n = stream
            .read(&mut chunk)
            .await
            .map_err(|_| AppError::msg("https_proxy_connect_failed"))?;
        if n == 0 || buffer.len() + n > MAX_RESPONSE_HEADER_BYTES {
            return Err(AppError::msg("https_proxy_connect_failed"));
        }
        buffer.extend_from_slice(&chunk[..n]);
    }
    if buffer.starts_with(b"HTTP/1.1 407") || buffer.starts_with(b"HTTP/1.0 407") {
        return Err(AppError::msg("https_proxy_auth_unsupported"));
    }
    if !buffer.starts_with(b"HTTP/1.1 2") && !buffer.starts_with(b"HTTP/1.0 2") {
        return Err(AppError::msg("https_proxy_connect_failed"));
    }
    Ok(stream)
}

async fn socks5_connect(mut stream: TcpStream, target: IpAddr, port: u16) -> AppResult<TcpStream> {
    stream
        .write_all(&[5, 1, 0])
        .await
        .map_err(|_| AppError::msg("https_proxy_unreachable"))?;
    let mut hello = [0; 2];
    stream
        .read_exact(&mut hello)
        .await
        .map_err(|_| AppError::msg("https_proxy_unreachable"))?;
    if hello[1] == 2 {
        return Err(AppError::msg("https_proxy_auth_unsupported"));
    }
    if hello != [5, 0] {
        return Err(AppError::msg("https_proxy_unsupported"));
    }
    let mut request = vec![5, 1, 0];
    match target {
        IpAddr::V4(ip) => {
            request.push(1);
            request.extend_from_slice(&ip.octets());
        }
        IpAddr::V6(ip) => {
            request.push(4);
            request.extend_from_slice(&ip.octets());
        }
    }
    request.extend_from_slice(&port.to_be_bytes());
    stream
        .write_all(&request)
        .await
        .map_err(|_| AppError::msg("https_proxy_connect_failed"))?;
    let mut head = [0; 4];
    stream
        .read_exact(&mut head)
        .await
        .map_err(|_| AppError::msg("https_proxy_connect_failed"))?;
    if head[1] != 0 {
        return Err(AppError::msg("https_proxy_connect_failed"));
    }
    let remaining = match head[3] {
        1 => 6,
        4 => 18,
        3 => {
            let mut n = [0; 1];
            stream
                .read_exact(&mut n)
                .await
                .map_err(|_| AppError::msg("https_proxy_connect_failed"))?;
            n[0] as usize + 2
        }
        _ => return Err(AppError::msg("https_proxy_connect_failed")),
    };
    let mut tail = vec![0; remaining];
    stream
        .read_exact(&mut tail)
        .await
        .map_err(|_| AppError::msg("https_proxy_connect_failed"))?;
    Ok(stream)
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
    // 这是测试/非固定传输网门的直连 client。生产安全抓取改走
    // `fixed_https_get`：无代理时直连 pinned 地址；使用系统代理时把同一已
    // 校验地址写入 HTTP CONNECT / SOCKS5，绝不让代理端重新解析域名。
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

#[cfg(test)]
mod fixed_transport_tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn http_connect_uses_ip_literal_authority() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut received = vec![0; 256];
            let n = socket.read(&mut received).await.unwrap();
            socket
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await
                .unwrap();
            String::from_utf8_lossy(&received[..n]).to_string()
        });
        let stream = TcpStream::connect(address).await.unwrap();
        let _ = http_connect(stream, "203.0.113.8".parse().unwrap(), 443)
            .await
            .unwrap();
        let request = server.await.unwrap();
        assert!(request.starts_with("CONNECT 203.0.113.8:443 HTTP/1.1"));
        assert!(!request.contains("example.com"));
    }

    #[tokio::test]
    async fn http_connect_brackets_an_ipv6_literal_authority() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut received = vec![0; 256];
            let n = socket.read(&mut received).await.unwrap();
            socket
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await
                .unwrap();
            String::from_utf8_lossy(&received[..n]).to_string()
        });
        let stream = TcpStream::connect(address).await.unwrap();
        let _ = http_connect(stream, "2001:db8::8".parse().unwrap(), 443)
            .await
            .unwrap();
        let request = server.await.unwrap();
        assert!(request.starts_with("CONNECT [2001:db8::8]:443 HTTP/1.1"));
    }

    #[tokio::test]
    async fn socks5_connect_uses_ip_atyp() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut hello = [0; 3];
            socket.read_exact(&mut hello).await.unwrap();
            socket.write_all(&[5, 0]).await.unwrap();
            let mut request = [0; 10];
            socket.read_exact(&mut request).await.unwrap();
            socket
                .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
                .await
                .unwrap();
            request
        });
        let stream = TcpStream::connect(address).await.unwrap();
        let _ = socks5_connect(stream, "203.0.113.8".parse().unwrap(), 443)
            .await
            .unwrap();
        let request = server.await.unwrap();
        assert_eq!(
            request[3], 1,
            "SOCKS5 target must use IPv4 ATYP, never domain"
        );
        assert_eq!(&request[4..8], &[203, 0, 113, 8]);
    }
}
