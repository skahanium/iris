//! 无边框 / Overlay 主窗口在各平台的圆角与标题壳层。
//!
//! - **Windows 11**：`transparent: false` + `shadow: true` → DWM 原生圆角（勿与 transparent 同开）。
//! - **macOS**：`titleBarStyle: Overlay` + `decorations: false` + 前端右侧自定义窗口控件；内部 title 设为 Iris。
//! - **Linux**：透明 WebView + 前端 CSS 裁切（尽力而为）。

use tauri::WebviewWindow;

/// 主窗口对外显示名（任务切换器 / 内部 title）；禁止保留 Tauri 模板默认「Tauri App」。
pub const MAIN_WINDOW_TITLE: &str = "Iris";

/// 与 `src/styles/globals.css` 中 `--window-radius` 保持一致（仅 macOS 壳层与跨平台单测引用）。
#[cfg(any(target_os = "macos", test))]
const WINDOW_CORNER_RADIUS: f64 = 12.0;

/// 为主窗口应用平台圆角与标题；失败时仅记录日志，不阻断启动。
pub fn apply_main_window_chrome(window: &WebviewWindow) {
    if let Err(error) = window.set_decorations(false) {
        tracing::warn!("窗口装饰未关闭: {error}");
    }

    if let Err(error) = window.set_title(MAIN_WINDOW_TITLE) {
        tracing::warn!("主窗口标题未设置为 Iris: {error}");
    }

    #[cfg(target_os = "macos")]
    {
        apply_macos_rounded_window(window);
    }

    #[cfg(windows)]
    {
        if let Err(error) = window.set_shadow(true) {
            tracing::warn!("Windows 窗口阴影/圆角未生效: {error}");
        }
    }
}

#[cfg(target_os = "macos")]
fn apply_macos_rounded_window(window: &WebviewWindow) {
    use tauri::window::{Effect, EffectState, EffectsBuilder};

    let effects = EffectsBuilder::new()
        .effect(Effect::WindowBackground)
        .state(EffectState::Active)
        .radius(WINDOW_CORNER_RADIUS)
        .build();

    if let Err(error) = window.set_effects(effects) {
        tracing::warn!("macOS 窗口圆角 effect 未生效: {error}");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn main_window_title_is_iris_not_tauri_default() {
        assert_eq!(super::MAIN_WINDOW_TITLE, "Iris");
        assert_ne!(super::MAIN_WINDOW_TITLE, "Tauri App");
    }

    #[test]
    fn window_corner_radius_matches_design_token() {
        assert_eq!(super::WINDOW_CORNER_RADIUS, 12.0);
    }

    #[test]
    fn default_titlebar_height_matches_chrome_metrics() {
        assert_eq!(crate::chrome_metrics::DEFAULT_TITLEBAR_HEIGHT, 44.0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_titlebar_height_matches_chrome_metrics() {
        assert_eq!(crate::chrome_metrics::MACOS_TITLEBAR_HEIGHT, 44.0);
    }
}
