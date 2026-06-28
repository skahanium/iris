//! 桌面壳层尺寸单一指标源（macOS / Windows / Linux 统一 44px）。
//!
//! 与前端 `globals.css`、`get_desktop_chrome_metrics` IPC 共用数值。

/// Windows / Linux 顶栏逻辑高度（px）。
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub const DEFAULT_TITLEBAR_HEIGHT: f64 = 44.0;

/// macOS Overlay 顶栏指标（仅 macOS 目标编译）。
#[cfg(target_os = "macos")]
pub mod macos {
    /// Overlay 顶栏逻辑高度（px），与 `html[data-iris-platform-macos]` 的 `2.75rem` 一致。
    pub const TITLEBAR_HEIGHT: f64 = 44.0;

    /// 系统原生红黄绿独占的左侧安全区（px）。
    pub const TRAFFIC_INSET: f64 = 88.0;
}

#[cfg(target_os = "macos")]
#[allow(unused_imports)]
pub use macos::TITLEBAR_HEIGHT as MACOS_TITLEBAR_HEIGHT;
#[cfg(target_os = "macos")]
#[allow(unused_imports)]
pub use macos::TRAFFIC_INSET as MACOS_TRAFFIC_INSET;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titlebar_height_default_is_44() {
        assert_eq!(DEFAULT_TITLEBAR_HEIGHT, 44.0);
    }

    #[cfg(target_os = "macos")]
    mod macos_tests {
        use super::{MACOS_TITLEBAR_HEIGHT, MACOS_TRAFFIC_INSET};

        #[test]
        fn titlebar_height_macos_is_44() {
            assert_eq!(MACOS_TITLEBAR_HEIGHT, 44.0);
        }

        #[test]
        fn traffic_inset_macos_reserves_native_lights() {
            assert_eq!(MACOS_TRAFFIC_INSET, 88.0);
        }
    }
}
