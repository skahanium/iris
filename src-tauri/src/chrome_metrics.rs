//! 桌面壳层尺寸单一指标源（macOS 32px / Windows·Linux 40px）。
//!
//! 与前端 `globals.css`、`get_desktop_chrome_metrics` IPC 及 `macos_traffic_lights` 共用数值。

/// Windows / Linux 顶栏逻辑高度（px）。
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub const DEFAULT_TITLEBAR_HEIGHT: f64 = 40.0;

/// macOS Overlay 顶栏与交通灯布局指标（仅 macOS 目标编译）。
#[cfg(target_os = "macos")]
pub mod macos {
    /// Overlay 顶栏逻辑高度（px），与 `html[data-iris-platform-macos]` 的 `2rem` 一致。
    pub const TITLEBAR_HEIGHT: f64 = 32.0;

    /// 交通灯距窗口左缘（logical pt）。
    pub const TRAFFIC_LIGHT_X: f64 = 12.0;

    /// 典型交通灯钮直径（logical pt），用于居中公式与默认 inset。
    pub const TRAFFIC_BUTTON_HEIGHT: f64 = 12.0;

    /// 末颗交通灯右侧至 Tab 内容区的留白（logical pt）。
    pub const TRAFFIC_TRAILING_PADDING: f64 = 8.0;

    /// 交通灯间距缺省值（close→miniaturize 中心距减钮宽），与实测约 8pt 一致。
    pub const TRAFFIC_SPACE_BETWEEN_DEFAULT: f64 = 8.0;

    /// 在 `target_height` 标题容器内垂直居中交通灯钮时的 `y` 偏移（logical pt）。
    pub const fn button_center_y_offset(button_height: f64, target_height: f64) -> f64 {
        (target_height - button_height) / 2.0
    }

    /// 根据实测或典型布局计算前端 `padding-left`（logical px）。
    pub fn traffic_inset_from_layout(
        x: f64,
        button_width: f64,
        space_between: f64,
        button_count: u32,
    ) -> f64 {
        let n = f64::from(button_count);
        x + n * button_width + (n - 1.0).max(0.0) * space_between + TRAFFIC_TRAILING_PADDING
    }

    /// 三键默认布局下的交通灯区宽度（72px ≈ `4.5rem`）。
    pub fn traffic_inset_default() -> f64 {
        traffic_inset_from_layout(
            TRAFFIC_LIGHT_X,
            TRAFFIC_BUTTON_HEIGHT,
            TRAFFIC_SPACE_BETWEEN_DEFAULT,
            3,
        )
    }
}

#[cfg(target_os = "macos")]
pub use macos::{
    TITLEBAR_HEIGHT as MACOS_TITLEBAR_HEIGHT, TRAFFIC_BUTTON_HEIGHT as MACOS_TRAFFIC_BUTTON_HEIGHT,
    TRAFFIC_LIGHT_X, TRAFFIC_SPACE_BETWEEN_DEFAULT as MACOS_TRAFFIC_SPACE_BETWEEN_DEFAULT,
    TRAFFIC_TRAILING_PADDING as MACOS_TRAFFIC_TRAILING_PADDING,
    button_center_y_offset, traffic_inset_default as macos_traffic_inset_default,
    traffic_inset_from_layout as macos_traffic_inset_from_layout,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titlebar_height_default_is_40() {
        assert_eq!(DEFAULT_TITLEBAR_HEIGHT, 40.0);
    }

    #[cfg(target_os = "macos")]
    mod macos_tests {
        use super::macos;
        use super::{MACOS_TITLEBAR_HEIGHT, macos_traffic_inset_default};

        #[test]
        fn titlebar_height_macos_is_32() {
            assert_eq!(MACOS_TITLEBAR_HEIGHT, 32.0);
        }

        #[test]
        fn button_center_y_for_12px_button_in_32px_bar() {
            assert_eq!(
                macos::button_center_y_offset(
                    macos::TRAFFIC_BUTTON_HEIGHT,
                    MACOS_TITLEBAR_HEIGHT,
                ),
                10.0
            );
        }

        #[test]
        fn macos_traffic_inset_default_is_72() {
            assert_eq!(macos_traffic_inset_default(), 72.0);
        }
    }
}
