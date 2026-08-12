//! 公共 HTTPS 出站校验与 DNS pinning（SSRF 防御）。
//!
//! 统一承担三类能力，供网页抓取与订阅获取共用，禁止复制第二套地址判断：
//! - `validate_https_url` / `validate_redirect_target`：URL 级完整校验；
//! - `resolve_public_addrs` / `validate_resolved_addrs`：DNS 解析后全部地址
//!   必须为公网，任何一条解析地址被拒绝都拒绝该主机；
//! - `build_pinned_client`：把本次连接固定到已校验地址（防 DNS rebinding）。

use std::net::{IpAddr, SocketAddr};

use reqwest::Client;

use crate::error::{AppError, AppResult};
use crate::network::cert_pinning::https_client_builder;

/// URL 级完整校验：仅 HTTPS、无 userinfo、拒绝 localhost / IPv4 / IPv6 私网 /
/// link-local / metadata / 保留段与私网域名提示（DNS rebinding 提示）。
pub fn validate_https_url(url: &str) -> AppResult<()> {
    crate::security::ipc_policy::validate_https_url(url)?;
    let host = host_of(url).ok_or_else(|| AppError::msg("无法解析 URL 主机名"))?;
    let host_lower = host.to_lowercase();
    if host_lower == "localhost" || host_lower.ends_with(".localhost") {
        return Err(AppError::msg("不允许访问本地主机"));
    }
    if host_lower == "0.0.0.0" {
        return Err(AppError::msg("不允许访问该地址"));
    }
    if let Ok(ip) = host_lower.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err(AppError::msg("不允许访问内网或保留地址"));
        }
        return Err(AppError::msg("仅允许域名 URL，不支持直接 IP 访问"));
    }
    if is_private_host_hint(&host_lower) {
        return Err(AppError::msg("不允许访问内网或保留地址"));
    }
    Ok(())
}

/// 重定向目标校验：与初始 URL 相同标准（绝对 HTTPS + 完整地址校验）。
/// 每次跳转后调用方必须重新解析并重新固定连接。
// 阶段 2 Task 2.2 `feed::fetch` 将消费这些能力；届时移除本标注。
#[allow(dead_code)]
pub fn validate_redirect_target(url: &str) -> AppResult<()> {
    validate_https_url(url)
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
// 阶段 2 Task 2.2 `feed::fetch` 将消费这些能力；届时移除本标注。
#[allow(dead_code)]
pub fn validate_resolved_addrs(addrs: &[IpAddr]) -> AppResult<Vec<IpAddr>> {
    if addrs.is_empty() {
        return Err(AppError::msg("DNS 解析结果为空"));
    }
    if addrs.iter().any(|ip| is_blocked_ip(*ip)) {
        return Err(AppError::msg("不允许访问内网或保留地址"));
    }
    Ok(addrs.to_vec())
}

/// 解析主机全部地址并整体校验（任何一条私网地址都拒绝该主机）。
// 阶段 2 Task 2.2 `feed::fetch` 将消费这些能力；届时移除本标注。
#[allow(dead_code)]
pub async fn resolve_public_addrs(host: &str) -> AppResult<Vec<IpAddr>> {
    let mut addrs: Vec<IpAddr> = Vec::new();
    for socket in tokio::net::lookup_host((host, 0))
        .await
        .map_err(|e| AppError::msg(format!("DNS 解析失败: {e}")))?
    {
        let ip = socket.ip();
        if !addrs.contains(&ip) {
            addrs.push(ip);
        }
    }
    validate_resolved_addrs(&addrs)
}

/// 构建固定到已校验地址的 HTTPS client；redirect policy 固定为 none，
/// 由调用方逐跳处理。代理策略与 TLS 配置复用 `cert_pinning`。
// 阶段 2 Task 2.2 `feed::fetch` 将消费这些能力；届时移除本标注。
#[allow(dead_code)]
pub fn build_pinned_client(host: &str, port: u16, addrs: &[IpAddr]) -> AppResult<Client> {
    let mut builder = https_client_builder().redirect(reqwest::redirect::Policy::none());
    for addr in addrs {
        builder = builder.resolve(host, SocketAddr::new(*addr, port));
    }
    builder
        .build()
        .map_err(|e| AppError::msg(format!("Failed to build pinned HTTP client: {e}")))
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
