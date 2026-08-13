//! Detect the active system / env proxy endpoint for UI status display.

use crate::network::proxy_policy::follow_system_proxy;

/// Human-readable status for the management-center proxy row.
pub fn proxy_status_label() -> String {
    proxy_status_label_for(follow_system_proxy(), detect_proxy_endpoint().as_deref())
}

/// Pure label helper (also used by unit tests).
pub(crate) fn proxy_status_label_for(follow: bool, endpoint: Option<&str>) -> String {
    if !follow {
        return "无代理".to_string();
    }
    match endpoint {
        Some(value) if !value.is_empty() => value.to_string(),
        _ => "无代理".to_string(),
    }
}

/// Detect the endpoint Iris would use when following system proxy.
///
/// Priority: `HTTPS_PROXY` → `HTTP_PROXY` → `ALL_PROXY` → OS HTTPS → OS HTTP → OS SOCKS.
pub fn detect_proxy_endpoint() -> Option<String> {
    for key in [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        if let Ok(value) = std::env::var(key) {
            if let Some(endpoint) = sanitize_proxy_display(&value) {
                return Some(endpoint);
            }
        }
    }
    detect_os_proxy_endpoint()
}

/// 供安全抓取读取的原始系统代理 URI；仅在系统设置可确定为 HTTP 时返回。
/// UI 仍只使用 [`detect_proxy_endpoint`] 的脱敏展示值。
pub(crate) fn detect_proxy_uri() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        detect_macos_proxy_uri()
    }
    #[cfg(windows)]
    {
        detect_windows_proxy_uri()
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        None
    }
}

/// 仅系统 PAC/自动发现存在、但没有可固定的显式代理端点时，安全抓取必须
/// 失败而非悄悄直连。环境变量的显式代理优先级更高，由调用方先处理。
pub(crate) fn system_proxy_requires_pac() -> bool {
    #[cfg(target_os = "macos")]
    {
        return macos_pac_enabled();
    }
    #[cfg(windows)]
    {
        return windows_pac_enabled();
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        false
    }
}

#[cfg(target_os = "macos")]
fn macos_pac_enabled() -> bool {
    use system_configuration::core_foundation::number::CFNumber;
    use system_configuration::core_foundation::string::CFString;
    use system_configuration::dynamic_store::SCDynamicStoreBuilder;

    let Some(store) = SCDynamicStoreBuilder::new("iris-safe-proxy-pac").build() else {
        return false;
    };
    let Some(proxies) = store.get_proxies() else {
        return false;
    };
    let enabled = |key: &str| {
        proxies
            .find(CFString::new(key))
            .and_then(|value| value.downcast::<CFNumber>())
            .and_then(|value| value.to_i32())
            .unwrap_or(0)
            == 1
    };
    enabled("ProxyAutoConfigEnable") || enabled("ProxyAutoDiscoveryEnable")
}

#[cfg(windows)]
fn windows_pac_enabled() -> bool {
    let Ok(settings) = windows_registry::CURRENT_USER
        .open("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
    else {
        return false;
    };
    let auto_config = settings
        .get_string("AutoConfigURL")
        .ok()
        .is_some_and(|value| !value.trim().is_empty());
    auto_config || settings.get_u32("AutoDetect").unwrap_or(0) != 0
}

#[cfg(target_os = "macos")]
fn detect_macos_proxy_uri() -> Option<String> {
    use system_configuration::core_foundation::base::CFType;
    use system_configuration::core_foundation::dictionary::CFDictionary;
    use system_configuration::core_foundation::string::CFString;
    use system_configuration::dynamic_store::SCDynamicStoreBuilder;

    let store = SCDynamicStoreBuilder::new("iris-safe-proxy").build()?;
    let proxies: CFDictionary<CFString, CFType> = store.get_proxies()?;
    for (scheme, enable_key, host_key, port_key) in [
        ("http", "HTTPSEnable", "HTTPSProxy", "HTTPSPort"),
        ("http", "HTTPEnable", "HTTPProxy", "HTTPPort"),
        ("socks5", "SOCKSEnable", "SOCKSProxy", "SOCKSPort"),
    ] {
        if let Some(endpoint) = read_macos_proxy(&proxies, enable_key, host_key, port_key) {
            return Some(format!("{scheme}://{endpoint}"));
        }
    }
    None
}

#[cfg(windows)]
fn detect_windows_proxy_uri() -> Option<String> {
    let settings = windows_registry::CURRENT_USER
        .open("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        .ok()?;
    if settings.get_u32("ProxyEnable").unwrap_or(0) == 0 {
        return None;
    }
    let server = settings.get_string("ProxyServer").ok()?;
    if !server.contains('=') {
        return sanitize_proxy_display(&server).map(|endpoint| format!("http://{endpoint}"));
    }
    for wanted in ["https", "http", "socks"] {
        for part in server.split(';') {
            let Some((scheme, value)) = part.trim().split_once('=') else {
                continue;
            };
            if scheme.eq_ignore_ascii_case(wanted) {
                let uri_scheme = if wanted == "socks" { "socks5" } else { "http" };
                return sanitize_proxy_display(value)
                    .map(|endpoint| format!("{uri_scheme}://{endpoint}"));
            }
        }
    }
    None
}

/// Strip scheme / userinfo / path; keep `host:port` (never show credentials).
pub(crate) fn sanitize_proxy_display(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let without_scheme = match trimmed.split_once("://") {
        Some((_, rest)) => rest,
        None => trimmed,
    };
    let without_auth = match without_scheme.rsplit_once('@') {
        Some((_, host_port)) => host_port,
        None => without_scheme,
    };
    let endpoint = without_auth
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(without_auth)
        .trim();
    if endpoint.is_empty() {
        None
    } else {
        Some(endpoint.to_string())
    }
}

#[cfg(target_os = "macos")]
fn detect_os_proxy_endpoint() -> Option<String> {
    use system_configuration::core_foundation::base::CFType;
    use system_configuration::core_foundation::dictionary::CFDictionary;
    use system_configuration::core_foundation::string::CFString;
    use system_configuration::dynamic_store::SCDynamicStoreBuilder;

    let store = SCDynamicStoreBuilder::new("iris-proxy-status").build()?;
    let proxies: CFDictionary<CFString, CFType> = store.get_proxies()?;

    for (enable_key, host_key, port_key) in [
        ("HTTPSEnable", "HTTPSProxy", "HTTPSPort"),
        ("HTTPEnable", "HTTPProxy", "HTTPPort"),
        ("SOCKSEnable", "SOCKSProxy", "SOCKSPort"),
    ] {
        if let Some(endpoint) = read_macos_proxy(&proxies, enable_key, host_key, port_key) {
            return Some(endpoint);
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn read_macos_proxy(
    proxies: &system_configuration::core_foundation::dictionary::CFDictionary<
        system_configuration::core_foundation::string::CFString,
        system_configuration::core_foundation::base::CFType,
    >,
    enable_key: &str,
    host_key: &str,
    port_key: &str,
) -> Option<String> {
    use system_configuration::core_foundation::number::CFNumber;
    use system_configuration::core_foundation::string::CFString;

    let enabled = proxies
        .find(CFString::new(enable_key))
        .and_then(|flag| flag.downcast::<CFNumber>())
        .and_then(|flag| flag.to_i32())
        .unwrap_or(0)
        == 1;
    if !enabled {
        return None;
    }
    let host = proxies
        .find(CFString::new(host_key))
        .and_then(|value| value.downcast::<CFString>())
        .map(|value| value.to_string())?;
    let port = proxies
        .find(CFString::new(port_key))
        .and_then(|value| value.downcast::<CFNumber>())
        .and_then(|value| value.to_i32());
    match port {
        Some(port) if port > 0 => Some(format!("{host}:{port}")),
        _ if !host.is_empty() => Some(host),
        _ => None,
    }
}

#[cfg(windows)]
fn detect_os_proxy_endpoint() -> Option<String> {
    let settings = windows_registry::CURRENT_USER
        .open("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        .ok()?;
    if settings.get_u32("ProxyEnable").unwrap_or(0) == 0 {
        return None;
    }
    let server = settings.get_string("ProxyServer").ok()?;
    // Windows may use `http=host:port;https=host:port` or a single `host:port`.
    if server.contains('=') {
        for part in server.split(';') {
            let part = part.trim();
            if let Some((scheme, value)) = part.split_once('=') {
                if scheme.eq_ignore_ascii_case("https") {
                    if let Some(endpoint) = sanitize_proxy_display(value) {
                        return Some(endpoint);
                    }
                }
            }
        }
        for part in server.split(';') {
            let part = part.trim();
            if let Some((_, value)) = part.split_once('=') {
                if let Some(endpoint) = sanitize_proxy_display(value) {
                    return Some(endpoint);
                }
            }
        }
        None
    } else {
        sanitize_proxy_display(&server)
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
fn detect_os_proxy_endpoint() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_scheme_auth_and_path() {
        assert_eq!(
            sanitize_proxy_display("http://user:secret@127.0.0.1:7890/path"),
            Some("127.0.0.1:7890".to_string())
        );
        assert_eq!(
            sanitize_proxy_display("socks5://127.0.0.1:7891"),
            Some("127.0.0.1:7891".to_string())
        );
        assert_eq!(
            sanitize_proxy_display("127.0.0.1:7890"),
            Some("127.0.0.1:7890".to_string())
        );
        assert_eq!(sanitize_proxy_display("   "), None);
    }

    #[test]
    fn label_is_none_when_follow_disabled() {
        assert_eq!(
            proxy_status_label_for(false, Some("127.0.0.1:7890")),
            "无代理"
        );
        assert_eq!(proxy_status_label_for(true, None), "无代理");
        assert_eq!(
            proxy_status_label_for(true, Some("127.0.0.1:7890")),
            "127.0.0.1:7890"
        );
    }
}
