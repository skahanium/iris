//! Process-wide follow-system-proxy preference for all HTTPS exits.

use std::sync::atomic::{AtomicBool, Ordering};

use reqwest::ClientBuilder;

/// Default: follow OS system proxy / `HTTP(S)_PROXY` (Clash, V2Ray, etc.).
static FOLLOW_SYSTEM_PROXY: AtomicBool = AtomicBool::new(true);

/// Whether Iris HTTPS clients should use the system / env proxy matcher.
pub fn follow_system_proxy() -> bool {
    FOLLOW_SYSTEM_PROXY.load(Ordering::Relaxed)
}

/// Update the in-process preference. Prefer [`crate::network::set_follow_system_proxy`]
/// so cached HTTPS clients are invalidated together.
pub fn store_follow_system_proxy(follow: bool) {
    FOLLOW_SYSTEM_PROXY.store(follow, Ordering::SeqCst);
}

/// Apply the current proxy preference to a reqwest builder.
///
/// When `follow_system_proxy` is false, forces direct connections via
/// [`ClientBuilder::no_proxy`]. When true, leaves the default system matcher
/// (requires the `system-proxy` Cargo feature).
pub fn apply_proxy_policy(builder: ClientBuilder) -> ClientBuilder {
    if follow_system_proxy() {
        builder
    } else {
        builder.no_proxy()
    }
}

/// Parse a settings JSON value for `follow_system_proxy` (missing / non-bool → true).
pub fn parse_follow_system_proxy_setting(value: Option<&serde_json::Value>) -> bool {
    match value {
        Some(serde_json::Value::Bool(v)) => *v,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_following_system_proxy() {
        let previous = follow_system_proxy();
        store_follow_system_proxy(true);
        assert!(follow_system_proxy());
        assert!(parse_follow_system_proxy_setting(None));
        assert!(parse_follow_system_proxy_setting(Some(
            &serde_json::Value::Null
        )));
        assert!(!parse_follow_system_proxy_setting(Some(
            &serde_json::json!(false)
        )));
        assert!(parse_follow_system_proxy_setting(Some(&serde_json::json!(
            true
        ))));
        store_follow_system_proxy(previous);
    }

    #[test]
    fn store_follow_system_proxy_updates_cache() {
        let previous = follow_system_proxy();
        store_follow_system_proxy(false);
        assert!(!follow_system_proxy());
        store_follow_system_proxy(true);
        assert!(follow_system_proxy());
        store_follow_system_proxy(previous);
    }
}
