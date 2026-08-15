//! Shared protection for cache files that are actively being written.
//!
//! Image and document downloads write to UUID-named `.part` files. Cleanup
//! paths in this process must never remove a partial that a live download is
//! writing, even when the download future is suspended at an await point.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use crate::error::{AppError, AppResult};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CacheFileOutcome {
    pub removed: u32,
    pub skipped_active: u32,
}

pub(crate) struct ActivePathSet {
    paths: Mutex<HashSet<PathBuf>>,
}

impl ActivePathSet {
    pub(crate) fn get_or_init(cell: &'static OnceLock<ActivePathSet>) -> &'static ActivePathSet {
        cell.get_or_init(|| ActivePathSet {
            paths: Mutex::new(HashSet::new()),
        })
    }

    pub(crate) fn protect(
        &'static self,
        path: PathBuf,
        error_code: &'static str,
    ) -> AppResult<ActivePathGuard> {
        self.paths
            .lock()
            .map_err(|_| AppError::msg(error_code))?
            .insert(path.clone());
        Ok(ActivePathGuard { path, active: self })
    }

    pub(crate) fn snapshot(&self) -> AppResult<HashSet<PathBuf>> {
        self.paths
            .lock()
            .map(|paths| paths.clone())
            .map_err(|_| AppError::msg("feed_cache_state_failed"))
    }
}

pub(crate) struct ActivePathGuard {
    path: PathBuf,
    active: &'static ActivePathSet,
}

impl Drop for ActivePathGuard {
    fn drop(&mut self) {
        if let Ok(mut paths) = self.active.paths.lock() {
            paths.remove(&self.path);
        }
    }
}

/// A `.part` file is safe to reap after this age. Active downloads refresh the
/// file mtime continuously while writing, and in-process downloads are also
/// registered in [`ActivePathSet`].
pub(crate) const PARTIAL_MAX_AGE: Duration = Duration::from_secs(6 * 60 * 60);

pub(crate) fn is_partial(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "part")
}

pub(crate) fn is_stale_partial(
    path: &Path,
    modified: SystemTime,
    now: SystemTime,
    max_age: Duration,
) -> bool {
    is_partial(path) && now.duration_since(modified).unwrap_or_default() > max_age
}
