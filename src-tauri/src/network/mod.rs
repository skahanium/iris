pub mod cert_pinning;
pub mod proxy_policy;
pub mod proxy_status;

pub use proxy_policy::{
    apply_proxy_policy, follow_system_proxy, parse_follow_system_proxy_setting,
};
pub use proxy_status::proxy_status_label;

/// Update the system-proxy preference and drop cached HTTPS clients.
pub fn set_follow_system_proxy(follow: bool) {
    let previous = proxy_policy::follow_system_proxy();
    proxy_policy::store_follow_system_proxy(follow);
    if previous != follow {
        cert_pinning::invalidate_https_clients();
    }
}
