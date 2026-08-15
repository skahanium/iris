//! Iris-owned temporary-file governance.
//!
//! Temporary directories are intentionally treated as hostile to recursive
//! deletion: an environment override may point anywhere, and third-party
//! runtimes write into the process `TMPDIR`. This module therefore requires an
//! ownership marker for user-initiated cleanup and only deletes files that are
//! either expired or old enough to be evicted by the size budget.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::error::{AppError, AppResult};
use crate::security::secure_delete::secure_delete;

pub(crate) const DEFAULT_TEMP_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
pub(crate) const DEFAULT_TEMP_MAX_BYTES: u64 = 512 * 1024 * 1024;
// Files younger than this are never evicted by the size budget.
pub(crate) const TEMP_CAP_MIN_AGE: Duration = Duration::from_secs(60 * 60);
pub(crate) const TEMP_OWNER_MARKER: &str = ".iris-temp-owner";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TempSweepReport {
    pub scanned_files: usize,
    pub deleted_files: usize,
    pub freed_bytes: u64,
    pub skipped_active: usize,
    pub remaining_bytes: u64,
}

pub(crate) struct TempSweepConfig<'a> {
    pub now: SystemTime,
    pub max_age: Duration,
    pub max_bytes: u64,
    pub cap_min_age: Duration,
    pub secure_delete: bool,
    pub modified_time: &'a dyn Fn(&Path) -> SystemTime,
}

fn owner_marker(root: &Path) -> PathBuf {
    root.join(TEMP_OWNER_MARKER)
}

/// Adopt `root` as an Iris-owned temporary directory.
///
/// Explicitly configured directories must be empty, already carry the
/// ownership marker, or be a proper child of the Iris home. The home-child
/// exception keeps development and portable installs working while still
/// refusing shared directories such as `/tmp`.
pub(crate) fn prepare_owned_dir(
    root: &Path,
    explicit: bool,
    allow_existing_home_child: bool,
) -> AppResult<()> {
    fs::create_dir_all(root)?;
    if explicit && !allow_existing_home_child && !owner_marker(root).exists() {
        let mut entries = fs::read_dir(root)?;
        if entries.next().is_some() {
            return Err(AppError::msg(
                "IRIS_TEMP_DIR must be empty or an existing Iris temporary directory",
            ));
        }
    }
    fs::write(owner_marker(root), b"iris temporary root\n")?;
    Ok(())
}

pub(crate) fn ensure_owned(root: &Path) -> AppResult<()> {
    if !root.exists() {
        return Err(AppError::msg("cache_temp_missing"));
    }
    if !owner_marker(root).is_file() {
        return Err(AppError::msg("cache_temp_not_owned"));
    }
    Ok(())
}

/// Refuse automatic cleanup for a populated directory without the ownership
/// marker. Empty or absent roots are left to their normal creation path.
pub(crate) fn ensure_owned_if_populated(root: &Path) -> AppResult<()> {
    if root.is_dir() && fs::read_dir(root)?.next().is_some() {
        ensure_owned(root)?;
    }
    Ok(())
}

/// Age- and budget-bounded sweep of an Iris-owned temporary root.
pub(crate) fn sweep(root: &Path, config: &TempSweepConfig<'_>) -> AppResult<TempSweepReport> {
    if !root.exists() || !root.is_dir() {
        return Ok(TempSweepReport::default());
    }

    let mut entries = Vec::new();
    collect_regular_files(root, config.modified_time, &mut entries)?;

    let mut report = TempSweepReport {
        scanned_files: entries.len(),
        ..TempSweepReport::default()
    };
    let now = config.now;

    // Expired files are removed unconditionally; recent files may be evicted by
    // the size budget only after `cap_min_age`.
    let mut remaining = Vec::new();
    for (path, modified, len) in entries {
        let age = now.duration_since(modified).unwrap_or_default();
        if age > config.max_age {
            remove_temp_file(&path, config.secure_delete)?;
            report.deleted_files += 1;
            report.freed_bytes = report.freed_bytes.saturating_add(len);
        } else {
            remaining.push((path, modified, len));
        }
    }

    let mut total: u64 = remaining.iter().map(|(_, _, len)| *len).sum();
    report.remaining_bytes = total;
    if total > config.max_bytes {
        remaining.sort_by_key(|(_, modified, _)| *modified);
        for (path, modified, len) in remaining {
            let age = now.duration_since(modified).unwrap_or_default();
            if age <= config.cap_min_age {
                report.skipped_active += 1;
                continue;
            }
            if total <= config.max_bytes {
                break;
            }
            remove_temp_file(&path, config.secure_delete)?;
            report.deleted_files += 1;
            report.freed_bytes = report.freed_bytes.saturating_add(len);
            total = total.saturating_sub(len);
        }
        report.remaining_bytes = total;
    }

    remove_empty_dirs(root, root)?;
    Ok(report)
}

fn collect_regular_files(
    root: &Path,
    modified_time: &dyn Fn(&Path) -> SystemTime,
    out: &mut Vec<(PathBuf, SystemTime, u64)>,
) -> AppResult<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            collect_regular_files(&path, modified_time, out)?;
        } else if metadata.is_file() {
            let modified = modified_time(&path);
            out.push((path, modified, metadata.len()));
        }
    }
    Ok(())
}

fn remove_temp_file(path: &Path, secure: bool) -> AppResult<()> {
    if secure {
        secure_delete(path)
    } else {
        fs::remove_file(path).map_err(Into::into)
    }
}

fn remove_empty_dirs(path: &Path, root: &Path) -> AppResult<()> {
    if path == root {
        return Ok(());
    }
    if !path.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        if fs::symlink_metadata(&child)?.file_type().is_dir() {
            remove_empty_dirs(&child, root)?;
        }
    }
    if fs::read_dir(path)?.next().is_none() {
        let _ = fs::remove_dir(path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_owned_dir_rejects_explicit_shared_root() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("foreign.tmp"), b"other app").unwrap();
        let error = prepare_owned_dir(dir.path(), true, false).unwrap_err();
        assert!(error.to_string().contains("empty or an existing Iris"));
    }

    #[test]
    fn prepare_owned_dir_adopts_default_non_empty_root() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("legacy.tmp"), b"legacy").unwrap();
        prepare_owned_dir(dir.path(), false, false).unwrap();
        assert!(owner_marker(dir.path()).is_file());
    }

    #[test]
    fn prepare_owned_dir_adopts_existing_home_child_for_development() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let temp = home.join("tmp");
        fs::create_dir_all(&temp).unwrap();
        fs::write(temp.join("legacy.tmp"), b"legacy").unwrap();

        prepare_owned_dir(&temp, true, true).unwrap();

        assert!(owner_marker(&temp).is_file());
        assert!(temp.join("legacy.tmp").exists());
    }

    #[test]
    fn sweep_removes_expired_files_but_keeps_recent_active_files() {
        let dir = tempfile::tempdir().unwrap();
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100 * 24 * 60 * 60);
        let old = now - Duration::from_secs(8 * 24 * 60 * 60);
        fs::write(dir.path().join("old.tmp"), b"old").unwrap();
        fs::write(dir.path().join("fresh.tmp"), b"fresh").unwrap();

        let report = sweep(
            dir.path(),
            &TempSweepConfig {
                now,
                max_age: Duration::from_secs(7 * 24 * 60 * 60),
                max_bytes: 64,
                cap_min_age: Duration::from_secs(60 * 60),
                secure_delete: false,
                modified_time: &move |path: &Path| {
                    if path.ends_with("old.tmp") {
                        old
                    } else {
                        now
                    }
                },
            },
        )
        .unwrap();

        assert_eq!(report.deleted_files, 1);
        assert!(!dir.path().join("old.tmp").exists());
        assert!(dir.path().join("fresh.tmp").exists());
    }

    #[test]
    fn sweep_evicts_oldest_but_skips_young_files_over_budget() {
        let dir = tempfile::tempdir().unwrap();
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100 * 24 * 60 * 60);
        let old = now - Duration::from_secs(2 * 60 * 60);
        fs::write(dir.path().join("old.tmp"), b"a".repeat(48)).unwrap();
        fs::write(dir.path().join("evict.tmp"), b"b".repeat(48)).unwrap();
        fs::write(dir.path().join("active.tmp"), b"c".repeat(48)).unwrap();

        let report = sweep(
            dir.path(),
            &TempSweepConfig {
                now,
                max_age: Duration::from_secs(7 * 24 * 60 * 60),
                max_bytes: 96,
                cap_min_age: Duration::from_secs(60 * 60),
                secure_delete: false,
                modified_time: &move |path: &Path| {
                    if path.ends_with("evict.tmp") {
                        old
                    } else {
                        now
                    }
                },
            },
        )
        .unwrap();

        assert!(report.remaining_bytes <= 96);
        assert!(dir.path().join("active.tmp").exists());
        assert_eq!(report.skipped_active, 2);
        assert_eq!(report.deleted_files, 1);
        assert!(!dir.path().join("evict.tmp").exists());
    }

    #[test]
    fn ensure_owned_rejects_unmarked_root() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("foreign.tmp"), b"x").unwrap();
        assert!(ensure_owned(dir.path()).is_err());
        prepare_owned_dir(dir.path(), false, false).unwrap();
        assert!(ensure_owned(dir.path()).is_ok());
    }
}
