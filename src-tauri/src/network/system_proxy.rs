//! 唯一“使用系统代理”设置的安全抓取快照。
//!
//! 此模块不新增任何用户设置：它只把现有环境/系统代理的当前状态投影成
//! 可审计的传输决策，供 RSS 与网页抓取把已校验 IP 固定写入 CONNECT/SOCKS。

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

pub(crate) fn snapshot_for(target: &Url) -> SystemProxySnapshot {
    if !follow_system_proxy() || bypasses(target) {
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
    let Ok(proxy) = Url::parse(&raw) else {
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

fn bypasses(target: &Url) -> bool {
    let host = target.host_str().unwrap_or_default();
    let port = target.port_or_known_default();
    let rules = std::env::var("NO_PROXY")
        .or_else(|_| std::env::var("no_proxy"))
        .unwrap_or_default();
    rules
        .split(',')
        .map(str::trim)
        .any(|rule| matches_no_proxy_rule(rule, host, port))
}

/// 匹配常见 `NO_PROXY` 规则。目标仍会先经过公网 DNS 校验；旁路只决定
/// 传输是否直连，绝不放宽 HTTPS、DNS pinning 或私网地址拒绝。
fn matches_no_proxy_rule(rule: &str, host: &str, port: Option<u16>) -> bool {
    if rule == "*" {
        return true;
    }
    let rule = rule.trim().trim_end_matches('.');
    if rule.is_empty() {
        return false;
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
            Some(443)
        ));
        assert!(matches_no_proxy_rule(
            "example.com",
            "api.example.com",
            Some(443)
        ));
        assert!(matches_no_proxy_rule(
            ".example.com",
            "api.example.com",
            Some(443)
        ));
        assert!(matches_no_proxy_rule(
            "example.com:8443",
            "example.com",
            Some(8443)
        ));
        assert!(!matches_no_proxy_rule(
            "example.com:8443",
            "example.com",
            Some(443)
        ));
        assert!(matches_no_proxy_rule("*", "anything.invalid", Some(443)));
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
