//! 无边框主窗口在各平台的圆角壳层。
//!
//! - **Windows 11**：`transparent: false` + `shadow: true` → DWM 原生圆角（勿与 transparent 同开）。
//! - **macOS**：`transparent: true` + `set_effects` 的 `radius`（与 `--window-radius` 一致）。
//! - **Linux**：透明 WebView + 前端 CSS 裁切（尽力而为）。

use tauri::WebviewWindow;

/// 与 `src/styles/globals.css` 中 `--window-radius` 保持一致（仅 macOS 壳层与跨平台单测引用）。
#[cfg(any(target_os = "macos", test))]
const WINDOW_CORNER_RADIUS: f64 = 12.0;

/// 为主窗口应用平台圆角；失败时仅记录日志，不阻断启动。
pub fn apply_main_window_chrome(window: &WebviewWindow) {
    #[cfg(windows)]
    {
        if let Err(error) = window.set_shadow(true) {
            tracing::warn!("Windows 窗口阴影/圆角未生效: {error}");
        }
    }

    #[cfg(target_os = "macos")]
    {
        apply_macos_rounded_window(window);
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
    fn window_corner_radius_matches_design_token() {
        assert_eq!(super::WINDOW_CORNER_RADIUS, 12.0);
    }
}
