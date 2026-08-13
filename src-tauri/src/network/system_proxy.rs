//! 唯一“使用系统代理”设置的安全抓取快照。
//!
//! 此模块不新增任何用户设置：它只把现有环境/系统代理的当前状态投影成
//! 可审计的传输决策，供 RSS 与网页抓取把已校验 IP 固定写入 CONNECT/SOCKS。

use std::net::IpAddr;

use reqwest::Url;

use crate::network::proxy_policy::follow_system_proxy;
use crate::network::proxy_status;

#[derive(Debug, Clone)]
pub(crate) enum SystemProxySnapshot {
    Direct,
    Http(Url),
    Socks5(Url),
    Unsupported(&'static str),
}

pub(crate) fn snapshot_for(target: &Url, resolved_addrs: &[IpAddr]) -> SystemProxySnapshot {
    if !follow_system_proxy() || bypasses(target, resolved_addrs) {
        return SystemProxySnapshot::Direct;
    }
    classify_proxy(
        configured_proxy().as_deref(),
        proxy_status::system_proxy_requires_pac(),
    )
}

fn classify_proxy(raw: Option<&str>, pac_configured: bool) -> SystemProxySnapshot {
    let Some(raw) = raw else {
        return if pac_configured {
            SystemProxySnapshot::Unsupported("https_proxy_unsupported")
        } else {
            SystemProxySnapshot::Direct
        };
    };
    let Ok(proxy) = Url::parse(raw) else {
        return SystemProxySnapshot::Unsupported("https_proxy_unsupported");
    };
    if !proxy.username().is_empty() || proxy.password().is_some() {
        return SystemProxySnapshot::Unsupported("https_proxy_auth_unsupported");
    }
    match proxy.scheme() {
        "http" => SystemProxySnapshot::Http(proxy),
        "socks5" => SystemProxySnapshot::Socks5(proxy),
        "socks5h" => SystemProxySnapshot::Unsupported("https_proxy_unsupported"),
        "https" => SystemProxySnapshot::Unsupported("https_proxy_unsupported"),
        _ => SystemProxySnapshot::Unsupported("https_proxy_unsupported"),
    }
}

fn configured_proxy() -> Option<String> {
    for key in [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                return Some(value);
            }
        }
    }
    crate::network::proxy_status::detect_proxy_uri()
}

fn bypasses(target: &Url, resolved_addrs: &[IpAddr]) -> bool {
    let host = target.host_str().unwrap_or_default();
    let port = target.port_or_known_default();
    let rules = std::env::var("NO_PROXY")
        .or_else(|_| std::env::var("no_proxy"))
        .unwrap_or_default();
    rules
        .split(',')
        .map(str::trim)
        .any(|rule| matches_no_proxy_rule(rule, host, port, resolved_addrs))
}

/// 匹配常见 `NO_PROXY` 规则。目标仍会先经过公网 DNS 校验；旁路只决定
/// 传输是否直连，绝不放宽 HTTPS、DNS pinning 或私网地址拒绝。
fn matches_no_proxy_rule(
    rule: &str,
    host: &str,
    port: Option<u16>,
    resolved_addrs: &[IpAddr],
) -> bool {
    if rule == "*" {
        return true;
    }
    let rule = rule.trim().trim_end_matches('.');
    if rule.is_empty() {
        return false;
    }
    if let Some((network, prefix)) = parse_cidr_rule(rule) {
        return resolved_addrs
            .iter()
            .any(|address| cidr_contains(network, prefix, *address));
    }
    if let Ok(address) = rule.parse::<IpAddr>() {
        return resolved_addrs.contains(&address);
    }
    let (rule_host, rule_port) = split_no_proxy_host_port(rule);
    if rule_port.is_some() && rule_port != port {
        return false;
    }
    let host = host.trim_end_matches('.');
    let rule_host = rule_host.trim_end_matches('.');
    if rule_host.starts_with('.') {
        return host
            .to_ascii_lowercase()
            .ends_with(&rule_host.to_ascii_lowercase());
    }
    host.eq_ignore_ascii_case(rule_host)
        || host
            .to_ascii_lowercase()
            .ends_with(&format!(".{}", rule_host.to_ascii_lowercase()))
}

fn parse_cidr_rule(rule: &str) -> Option<(IpAddr, u8)> {
    let (address, prefix) = rule.split_once('/')?;
    let address = address.parse::<IpAddr>().ok()?;
    let prefix = prefix.parse::<u8>().ok()?;
    let max = match address {
        IpAddr::V4(_) => 32,
        IpAddr::V6(_) => 128,
    };
    (prefix <= max).then_some((address, prefix))
}

fn cidr_contains(network: IpAddr, prefix: u8, address: IpAddr) -> bool {
    match (network, address) {
        (IpAddr::V4(network), IpAddr::V4(address)) => {
            let mask = if prefix == 0 {
                0
            } else {
                u32::MAX << (32 - prefix)
            };
            (u32::from(network) & mask) == (u32::from(address) & mask)
        }
        (IpAddr::V6(network), IpAddr::V6(address)) => {
            let network = u128::from(network);
            let address = u128::from(address);
            let mask = if prefix == 0 {
                0
            } else {
                u128::MAX << (128 - prefix)
            };
            (network & mask) == (address & mask)
        }
        _ => false,
    }
}

fn split_no_proxy_host_port(rule: &str) -> (&str, Option<u16>) {
    if let Some(bracket_end) = rule.find(']').filter(|_| rule.starts_with('[')) {
        let host = &rule[1..bracket_end];
        let port = rule
            .get(bracket_end + 1..)
            .and_then(|suffix| suffix.strip_prefix(':'))
            .and_then(|value| value.parse::<u16>().ok());
        return (host, port);
    }
    if rule.matches(':').count() == 1 {
        if let Some((host, port)) = rule.rsplit_once(':') {
            if let Ok(port) = port.parse::<u16>() {
                return (host, Some(port));
            }
        }
    }
    (rule, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypass_rules_are_literal_and_bounded() {
        assert!(!matches_no_proxy_rule(
            "example.invalid",
            "other.invalid",
            Some(443),
            &[]
        ));
        assert!(matches_no_proxy_rule(
            "example.com",
            "api.example.com",
            Some(443),
            &[]
        ));
        assert!(matches_no_proxy_rule(
            ".example.com",
            "api.example.com",
            Some(443),
            &[]
        ));
        assert!(matches_no_proxy_rule(
            "example.com:8443",
            "example.com",
            Some(8443),
            &[]
        ));
        assert!(!matches_no_proxy_rule(
            "example.com:8443",
            "example.com",
            Some(443),
            &[]
        ));
        assert!(matches_no_proxy_rule(
            "*",
            "anything.invalid",
            Some(443),
            &[]
        ));
        let address = "203.0.113.8".parse().unwrap();
        assert!(matches_no_proxy_rule(
            "203.0.113.8",
            "example.com",
            Some(443),
            &[address]
        ));
        assert!(matches_no_proxy_rule(
            "203.0.113.0/24",
            "example.com",
            Some(443),
            &[address]
        ));
        assert!(!matches_no_proxy_rule(
            "203.0.114.0/24",
            "example.com",
            Some(443),
            &[address]
        ));
    }

    #[test]
    fn ip_and_cidr_bypass_follow_the_pinned_connection_address() {
        let pinned = "203.0.114.8".parse().unwrap();
        let other_dns_answer = "203.0.113.8".parse().unwrap();

        assert!(matches_no_proxy_rule(
            "203.0.113.0/24",
            "example.com",
            Some(443),
            &[pinned, other_dns_answer]
        ));
        assert!(
            !matches_no_proxy_rule("203.0.113.0/24", "example.com", Some(443), &[pinned]),
            "旁路判断必须只看最终固定连接的 IP，不能由另一个 DNS 答案改变传输策略"
        );
    }

    #[test]
    fn pac_without_an_explicit_proxy_never_falls_back_to_direct() {
        assert!(matches!(
            classify_proxy(None, true),
            SystemProxySnapshot::Unsupported("https_proxy_unsupported")
        ));
    }

    #[test]
    fn http_and_socks5_are_the_only_supported_proxy_schemes() {
        assert!(matches!(
            classify_proxy(Some("http://127.0.0.1:7890"), false),
            SystemProxySnapshot::Http(_)
        ));
        assert!(matches!(
            classify_proxy(Some("socks5://127.0.0.1:7890"), false),
            SystemProxySnapshot::Socks5(_)
        ));
        assert!(matches!(
            classify_proxy(Some("https://127.0.0.1:7890"), false),
            SystemProxySnapshot::Unsupported("https_proxy_unsupported")
        ));
    }
}
