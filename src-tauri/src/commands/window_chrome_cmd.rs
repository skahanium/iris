use serde::Serialize;
use tauri::{AppHandle, WebviewWindow};

use crate::error::{AppError, AppResult};

/// 桌面顶栏指标（逻辑像素，与 `chrome_metrics` 一致）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopChromeMetrics {
    pub titlebar_height_logical: f64,
    pub traffic_inset_logical: f64,
    pub scale_factor: f64,
}

/// 返回当前平台的顶栏高度。macOS 原生红黄绿独占左侧安全区。
#[tauri::command]
pub fn get_desktop_chrome_metrics(window: WebviewWindow) -> DesktopChromeMetrics {
    let scale_factor = window.scale_factor().unwrap_or(1.0);

    #[cfg(target_os = "macos")]
    {
        DesktopChromeMetrics {
            titlebar_height_logical: crate::chrome_metrics::MACOS_TITLEBAR_HEIGHT,
            traffic_inset_logical: crate::chrome_metrics::MACOS_TRAFFIC_INSET,
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

/// Show the hidden startup window once the React splash has mounted.
#[tauri::command]
pub fn show_main_window_when_ready(window: WebviewWindow) -> AppResult<()> {
    crate::window_chrome::apply_main_window_chrome(&window);
    window
        .show()
        .map_err(|e| AppError::msg(format!("Failed to show main window: {e}")))?;

    #[cfg(target_os = "macos")]
    crate::window_chrome::apply_main_window_chrome(&window);

    let _ = window.set_focus();
    Ok(())
}

/// Reapply borderless window title and platform corner styling.
#[tauri::command]
pub fn reapply_window_chrome(window: WebviewWindow) {
    crate::window_chrome::apply_main_window_chrome(&window);
}

/// Exit the Tauri application after the frontend has completed close guards.
#[tauri::command]
pub fn app_exit(app: AppHandle) {
    app.exit(0);
}
