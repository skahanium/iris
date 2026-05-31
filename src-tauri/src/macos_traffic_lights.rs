//! macOS Overlay 标题栏交通灯定位（逻辑改编自 wry，Apache-2.0 / MIT）。
//!
//! Tauri 2.11 的 `WebviewWindow` 仅在创建时接受 `traffic_light_position`；
//! `set_title`、全屏/还原等会重置系统位置，需在运行时调用本模块重新 inset。

use tauri::WebviewWindow;

use crate::chrome_metrics::{self, MACOS_TITLEBAR_HEIGHT};

pub use crate::chrome_metrics::TRAFFIC_LIGHT_X;

/// 创建窗口时 `trafficLightPosition.y` 初值；运行时以 `inset_traffic_lights` 为准。
pub const TRAFFIC_LIGHT_Y: f64 = chrome_metrics::button_center_y_offset(
    chrome_metrics::MACOS_TRAFFIC_BUTTON_HEIGHT,
    MACOS_TITLEBAR_HEIGHT,
);

/// 将交通灯移至 32px 标题容器内垂直居中。
pub fn apply_traffic_light_position(window: &WebviewWindow) {
    let Ok(ns_window_ptr) = window.ns_window() else {
        tracing::warn!("无法获取 NSWindow，跳过交通灯定位");
        return;
    };
    if ns_window_ptr.is_null() {
        tracing::warn!("NSWindow 指针为空，跳过交通灯定位");
        return;
    }

    unsafe {
        use objc2::rc::Retained;

        let Some(ns_window) = Retained::retain(ns_window_ptr.cast()) else {
            tracing::warn!("无法 retain NSWindow");
            return;
        };

        inset_traffic_lights(&ns_window, TRAFFIC_LIGHT_X, MACOS_TITLEBAR_HEIGHT);
    }
}

/// 调整标准窗口按钮（交通灯）在自定义标题栏内的位置。
///
/// # Safety
///
/// `window` 必须为有效的 `NSWindow` 指针；仅由 `apply_traffic_light_position` 在
/// `ns_window()` 校验后调用。Tauri 未暴露运行时 `set_traffic_light_position`，故保留此 unsafe。
unsafe fn inset_traffic_lights(window: &objc2_app_kit::NSWindow, x: f64, target_height: f64) {
    use objc2_app_kit::{NSView, NSWindowButton};

    let Some(close) = window.standardWindowButton(NSWindowButton::CloseButton) else {
        tracing::warn!("未找到关闭按钮，跳过交通灯定位");
        return;
    };
    let Some(miniaturize) = window.standardWindowButton(NSWindowButton::MiniaturizeButton) else {
        tracing::warn!("未找到最小化按钮，跳过交通灯定位");
        return;
    };
    let zoom = window.standardWindowButton(NSWindowButton::ZoomButton);

    let title_bar_container_view = close.superview().unwrap().superview().unwrap();

    let close_rect = NSView::frame(&close);
    let button_height = close_rect.size.height;

    let mut title_bar_rect = NSView::frame(&title_bar_container_view);
    title_bar_rect.size.height = target_height;
    title_bar_rect.origin.y = window.frame().size.height - target_height;
    title_bar_container_view.setFrame(title_bar_rect);

    let space_between = NSView::frame(&miniaturize).origin.x - close_rect.origin.x;
    let vertical_offset = chrome_metrics::button_center_y_offset(button_height, target_height);
    debug_assert!(
        (vertical_offset - TRAFFIC_LIGHT_Y).abs() < f64::EPSILON
            || (button_height - chrome_metrics::MACOS_TRAFFIC_BUTTON_HEIGHT).abs() > f64::EPSILON,
        "traffic light y should match chrome_metrics when button height is nominal"
    );

    let mut window_buttons = vec![close, miniaturize];
    if let Some(zoom) = zoom {
        window_buttons.push(zoom);
    }

    for (i, button) in window_buttons.into_iter().enumerate() {
        let mut rect = NSView::frame(&button);
        rect.origin.x = x + (i as f64 * space_between);
        rect.origin.y = vertical_offset;
        button.setFrameOrigin(rect.origin);
    }
}
