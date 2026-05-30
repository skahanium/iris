use std::fs;
use std::path::Path;
use std::sync::Arc;

use tauri::State;

use crate::app::AppState;
use crate::error::{AppError, AppResult};

#[cfg(unix)]
const SENSITIVE_PREFIXES: &[&str] = &[
    "/etc/", "/usr/", "/var/", "/opt/", "/sbin/", "/bin/", "/lib/", "/lib64/", "/boot/", "/proc/",
    "/sys/", "/dev/", "/run/", "/snap/",
];

#[cfg(windows)]
const SENSITIVE_PREFIXES: &[&str] = &[
    "C:\\Windows\\",
    "C:\\Program Files\\",
    "C:\\Program Files (x86)\\",
    "C:\\ProgramData\\",
];

fn is_sensitive_system_path(path: &Path) -> bool {
    let normalized = path.to_string_lossy();
    let normalized = normalized.replace('\\', "/");
    let lower = normalized.to_lowercase();
    SENSITIVE_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(&prefix.to_lowercase()))
}

#[tauri::command]
pub fn export_file(
    _state: State<'_, Arc<AppState>>,
    dest_path: String,
    content: String,
) -> AppResult<()> {
    if dest_path.trim().is_empty() {
        return Err(AppError::msg("导出路径不能为空"));
    }
    let dest = std::path::PathBuf::from(&dest_path);
    if is_sensitive_system_path(&dest) {
        return Err(AppError::msg("不允许导出到系统目录"));
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&dest, &content)?;
    Ok(())
}
