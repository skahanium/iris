//! `network::safe_https` 公共 HTTPS 出站校验契约测试。
//!
//! 契约来源：原 `llm::fetch_web_page` 的地址校验测试（localhost、IPv4/IPv6
//! 私网、DNS 重绑定提示），另新增 metadata 地址、混合公共/私网 DNS、重定向
//! 目标再校验与 DNS pinning 测试。全部为纯函数/确定性测试，不发起网络请求。

use std::net::IpAddr;

use super::safe_https::{
    build_pinned_client_with_timeout, host_of, resolve_public_addrs, validate_https_url,
    validate_resolved_addrs, validate_response_headers, MAX_RESPONSE_HEADER_BYTES,
};

#[test]
fn validate_rejects_localhost() {
    assert_eq!(
        validate_https_url("https://localhost/x")
            .expect_err("localhost rejected")
            .to_string(),
        "https_url_private"
    );
    assert!(validate_https_url("https://api.localhost/x").is_err());
}

#[test]
fn validate_rejects_private_ip() {
    assert_eq!(
        validate_https_url("https://192.168.1.1/")
            .expect_err("private IP rejected")
            .to_string(),
        "https_url_private"
    );
    assert!(validate_https_url("https://10.0.0.1/").is_err());
}

#[test]
fn validate_rejects_ipv6_mapped() {
    assert!(validate_https_url("https://[::ffff:192.168.1.1]/").is_err());
}

#[test]
fn validate_rejects_ipv6_link_local() {
    assert!(validate_https_url("https://[fe80::1]/").is_err());
}

#[test]
fn validate_rejects_ipv6_loopback_with_port() {
    assert!(validate_https_url("https://[::1]:443/").is_err());
}

#[test]
fn validate_rejects_userinfo() {
    assert!(validate_https_url("https://user:pass@example.com/").is_err());
    assert!(validate_https_url("https://user@example.com/").is_err());
}

#[test]
fn validate_rejects_ipv6_ula() {
    assert!(validate_https_url("https://[fd00::1]/").is_err());
}

#[test]
fn validate_rejects_ipv6_translation() {
    assert!(validate_https_url("https://[64:ff9b::192.168.1.1]/").is_err());
}

#[test]
fn validate_rejects_cgnat() {
    assert!(validate_https_url("https://100.64.0.1/").is_err());
}

#[test]
fn validate_rejects_benchmark() {
    assert!(validate_https_url("https://198.18.0.1/").is_err());
}

#[test]
fn validate_rejects_dns_rebinding() {
    assert!(validate_https_url("https://192.168.1.1.nip.io/").is_err());
}

#[test]
fn validate_rejects_172_private() {
    assert!(validate_https_url("https://172.16.0.1/").is_err());
    assert!(validate_https_url("https://172.31.255.255/").is_err());
}

#[test]
fn validate_accepts_172_public() {
    // 172.32.x.x 是公网段——仅 172.16-31 为私网。
    validate_https_url("https://172.32.0.1.example.com/").unwrap();
}

#[test]
fn validate_rejects_local_suffix() {
    assert!(validate_https_url("https://myserver.local/").is_err());
    assert!(validate_https_url("https://intranet.internal/").is_err());
}

#[test]
fn validate_rejects_metadata_address() {
    // AWS/GCP/Azure metadata 端点（169.254.169.254）与 link-local 全段。
    assert!(validate_https_url("https://169.254.169.254/latest/meta-data/").is_err());
    assert!(validate_https_url("https://169.254.170.2/").is_err());
}

#[test]
fn validate_rejects_http_scheme() {
    assert!(validate_https_url("http://example.com/").is_err());
    assert!(validate_https_url("ftp://example.com/").is_err());
}

#[test]
fn validate_rejects_empty_and_broken() {
    assert!(validate_https_url("").is_err());
    assert!(validate_https_url("https://").is_err());
}

#[test]
fn validate_accepts_https_domain() {
    validate_https_url("https://www.example.com/doc").unwrap();
    validate_https_url("https://example.com:8443/feed.xml").unwrap();
}

#[test]
fn host_of_extracts_bare_host() {
    assert_eq!(
        host_of("https://example.com/path").as_deref(),
        Some("example.com")
    );
    assert_eq!(host_of("https://[::1]:443/x").as_deref(), Some("::1"));
    assert_eq!(
        host_of("https://user:pass@example.com/"),
        None,
        "userinfo 必须返回 None"
    );
    assert_eq!(host_of("not a url"), None);
}

#[test]
fn validate_resolved_addrs_rejects_mixed_public_private() {
    // 同一主机同时解析出公网与私网地址时必须整体拒绝（防 DNS rebinding）。
    let mixed = [
        "93.184.216.34".parse::<IpAddr>().unwrap(),
        "192.168.1.1".parse::<IpAddr>().unwrap(),
    ];
    assert_eq!(
        validate_resolved_addrs(&mixed)
            .expect_err("mixed result rejected")
            .to_string(),
        "https_dns_private"
    );
}

#[test]
fn validate_resolved_addrs_accepts_all_public() {
    let public = [
        "93.184.216.34".parse::<IpAddr>().unwrap(),
        "2606:2800:220:1:248:1893:25c8:1946"
            .parse::<IpAddr>()
            .unwrap(),
    ];
    let accepted = validate_resolved_addrs(&public).expect("all public accepted");
    assert_eq!(accepted.len(), 2);
}

#[test]
fn validate_resolved_addrs_rejects_empty() {
    assert_eq!(
        validate_resolved_addrs(&[])
            .expect_err("空解析结果必须报错")
            .to_string(),
        "https_dns_empty"
    );
}

#[test]
fn validate_resolved_addrs_rejects_single_private() {
    let private = ["127.0.0.1".parse::<IpAddr>().unwrap()];
    assert!(validate_resolved_addrs(&private).is_err());
}

#[test]
fn every_redirect_target_uses_the_full_https_validator() {
    // 重定向目标必须与初始请求一样通过完整校验。
    assert!(validate_https_url("http://example.com/redirect").is_err());
    assert!(validate_https_url("https://192.168.0.1/redirect").is_err());
    assert!(validate_https_url("relative/path").is_err());
    validate_https_url("https://www.example.com/redirect").unwrap();
}

#[tokio::test]
async fn resolve_public_addrs_rejects_localhost_without_network() {
    // localhost 解析必然命中 loopback：确定性拒绝，无需真实网络。
    assert!(resolve_public_addrs("localhost").await.is_err());
}

#[tokio::test]
async fn dns_failures_use_stable_non_sensitive_code() {
    let error = resolve_public_addrs("rss-secret.invalid")
        .await
        .expect_err("reserved invalid TLD must not resolve");
    assert_eq!(error.to_string(), "https_dns_failed");
    assert!(!error.to_string().contains("rss-secret"));
}

#[test]
fn response_header_budget_rejects_oversized_headers() {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "x-large",
        reqwest::header::HeaderValue::from_bytes(&vec![b'a'; MAX_RESPONSE_HEADER_BYTES])
            .expect("valid header"),
    );

    let error = validate_response_headers(&headers)
        .expect_err("header names and values together must stay within the budget");
    assert_eq!(error.to_string(), "https_response_headers_too_large");
}

#[test]
fn pinned_security_client_builds_without_proxy_delegation() {
    let addrs = ["93.184.216.34".parse::<IpAddr>().unwrap()];
    let client = build_pinned_client_with_timeout(
        "example.com",
        443,
        &addrs,
        std::time::Duration::from_secs(20),
    )
    .expect("direct pinned client builds");
    let _ = client;
}
