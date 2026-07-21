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
    reveal_main_window(&window)
}

pub(crate) fn reveal_main_window(window: &WebviewWindow) -> AppResult<()> {
    crate::window_chrome::apply_main_window_chrome(window);
    window
        .show()
        .map_err(|e| AppError::msg(format!("Failed to show main window: {e}")))?;

    #[cfg(target_os = "macos")]
    crate::window_chrome::apply_main_window_chrome(window);

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

/// Open an HTTPS URL in the system default browser.
///
/// Only `https://` is accepted. This is used by AI citation / source links so the
/// WebView never navigates away from the app.
#[tauri::command]
pub fn open_external_https_url(url: String) -> AppResult<()> {
    open_https_url_in_system_browser(&url)
}

fn open_https_url_in_system_browser(url: &str) -> AppResult<()> {
    let trimmed = url.trim();
    if !trimmed.to_ascii_lowercase().starts_with("https://") {
        return Err(AppError::msg("open_external_url_rejected: https_only"));
    }
    if trimmed.contains(['\n', '\r', '\0', '"']) {
        return Err(AppError::msg("open_external_url_rejected: invalid_url"));
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", trimmed])
            .spawn()
            .map_err(|error| AppError::msg(format!("open_external_url_failed: {error}")))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(trimmed)
            .spawn()
            .map_err(|error| AppError::msg(format!("open_external_url_failed: {error}")))?;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(trimmed)
            .spawn()
            .map_err(|error| AppError::msg(format!("open_external_url_failed: {error}")))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::open_https_url_in_system_browser;

    #[test]
    fn rejects_non_https_urls() {
        let err = open_https_url_in_system_browser("http://example.com").unwrap_err();
        assert!(err.to_string().contains("https_only"));
        let err = open_https_url_in_system_browser("javascript:alert(1)").unwrap_err();
        assert!(err.to_string().contains("https_only"));
    }
}
