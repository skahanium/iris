use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::error::AppResult;

pub const DEFAULT_TEMP_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
pub const DEFAULT_CACHE_MAX_AGE: Duration = Duration::from_secs(30 * 24 * 60 * 60);

pub struct CleanupConfig {
    pub temp_dirs: Vec<PathBuf>,
    pub cache_dirs: Vec<PathBuf>,
    pub now: SystemTime,
    pub temp_max_age: Duration,
    pub cache_max_age: Duration,
    pub modified_time: Box<dyn Fn(&Path) -> SystemTime>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CleanupReport {
    pub scanned_files: usize,
    pub deleted_files: usize,
    pub deleted_bytes: u64,
}

pub fn cleanup_from_environment() -> AppResult<CleanupReport> {
    if std::env::var("IRIS_AUTO_CLEANUP")
        .map(|value| value == "0" || value.eq_ignore_ascii_case("false"))
        .unwrap_or(false)
    {
        return Ok(CleanupReport::default());
    }

    let temp_dirs = env_path("IRIS_TEMP_DIR").into_iter().collect();
    let cache_dirs = env_path("IRIS_CACHE_DIR").into_iter().collect();

    cleanup_once(CleanupConfig {
        temp_dirs,
        cache_dirs,
        now: SystemTime::now(),
        temp_max_age: DEFAULT_TEMP_MAX_AGE,
        cache_max_age: DEFAULT_CACHE_MAX_AGE,
        modified_time: Box::new(|path| {
            fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .unwrap_or_else(|_| SystemTime::now())
        }),
    })
}

pub fn cleanup_once(config: CleanupConfig) -> AppResult<CleanupReport> {
    let mut report = CleanupReport::default();

    for dir in config.temp_dirs {
        clean_root(
            &dir,
            config.now,
            config.temp_max_age,
            true,
            &config.modified_time,
            &mut report,
        )?;
    }

    for dir in config.cache_dirs {
        clean_root(
            &dir,
            config.now,
            config.cache_max_age,
            false,
            &config.modified_time,
            &mut report,
        )?;
    }

    Ok(report)
}

fn clean_root(
    root: &Path,
    now: SystemTime,
    max_age: Duration,
    secure_expired_files: bool,
    modified_time: &dyn Fn(&Path) -> SystemTime,
    report: &mut CleanupReport,
) -> AppResult<()> {
    if !root.exists() || !root.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        clean_entry(
            &entry.path(),
            root,
            now,
            max_age,
            secure_expired_files,
            modified_time,
            report,
        )?;
    }

    Ok(())
}

fn clean_entry(
    path: &Path,
    root: &Path,
    now: SystemTime,
    max_age: Duration,
    secure_expired_files: bool,
    modified_time: &dyn Fn(&Path) -> SystemTime,
    report: &mut CleanupReport,
) -> AppResult<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            clean_entry(
                &entry.path(),
                root,
                now,
                max_age,
                secure_expired_files,
                modified_time,
                report,
            )?;
        }
        remove_empty_dir(path, root)?;
        return Ok(());
    }

    if !metadata.is_file() || is_protected_path(path) {
        return Ok(());
    }

    report.scanned_files += 1;
    let modified = modified_time(path);
    let expired = now
        .duration_since(modified)
        .map(|age| age > max_age)
        .unwrap_or(false);

    if expired {
        let bytes = metadata.len();
        if secure_expired_files {
            crate::security::secure_delete::secure_delete(path)?;
        } else {
            fs::remove_file(path)?;
        }
        report.deleted_files += 1;
        report.deleted_bytes += bytes;
    }

    Ok(())
}

fn remove_empty_dir(path: &Path, root: &Path) -> AppResult<()> {
    if path == root {
        return Ok(());
    }
    if fs::read_dir(path)?.next().is_none() {
        fs::remove_dir(path)?;
    }
    Ok(())
}

fn is_protected_path(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".md")
        || lower.ends_with(".db")
        || lower.ends_with(".sqlite")
        || lower.ends_with(".sqlite3")
        || lower.ends_with(".db-wal")
        || lower.ends_with(".db-shm")
        || lower.ends_with(".json")
        || lower.ends_with(".toml")
        || lower.ends_with(".key")
        || lower.ends_with(".pem")
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}
