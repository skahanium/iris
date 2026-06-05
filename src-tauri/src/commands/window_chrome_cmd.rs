use serde::Serialize;
use tauri::{AppHandle, WebviewWindow};

/// 桌面顶栏与交通灯留白指标（逻辑像素，与 `chrome_metrics` 一致）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopChromeMetrics {
    pub titlebar_height_logical: f64,
    pub traffic_inset_logical: f64,
    pub scale_factor: f64,
}

/// 返回当前平台的顶栏高度与 macOS 交通灯区宽度，供前端写入 CSS 变量。
#[tauri::command]
pub fn get_desktop_chrome_metrics(window: WebviewWindow) -> DesktopChromeMetrics {
    let scale_factor = window.scale_factor().unwrap_or(1.0);

    #[cfg(target_os = "macos")]
    {
        DesktopChromeMetrics {
            titlebar_height_logical: crate::chrome_metrics::MACOS_TITLEBAR_HEIGHT,
            traffic_inset_logical: crate::chrome_metrics::macos_traffic_inset_default(),
            scale_factor,
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
        DesktopChromeMetrics {
            titlebar_height_logical: crate::chrome_metrics::DEFAULT_TITLEBAR_HEIGHT,
            traffic_inset_logical: 0.0,
            scale_factor,
        }
    }
}

/// 前端在 resize / 全屏切换后调用，重新应用 macOS 交通灯位置与窗口标题。
#[tauri::command]
pub fn reapply_window_chrome(window: WebviewWindow) {
    crate::window_chrome::apply_main_window_chrome(&window);
}

/// Exit the Tauri application after the frontend has completed close guards.
#[tauri::command]
pub fn app_exit(app: AppHandle) {
    app.exit(0);
}
