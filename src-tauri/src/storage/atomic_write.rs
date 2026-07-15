use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::error::{AppError, AppResult};

struct TemporaryFileGuard {
    path: PathBuf,
    committed: bool,
}

impl TemporaryFileGuard {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            committed: false,
        }
    }
}

impl Drop for TemporaryFileGuard {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Durably replace a file using a unique sibling temporary file.
///
/// The previous target remains readable until the synced temporary file is
/// atomically renamed into place. Derived-index work must happen after this
/// function returns so index failures cannot invalidate an acknowledged note.
pub(crate) fn atomic_write(path: &Path, data: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::msg("atomic write target has no parent"))?;
    fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::msg("atomic write target has an invalid file name"))?;
    let temporary = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
    let mut guard = TemporaryFileGuard::new(temporary.clone());

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(data)?;
    file.sync_all()?;
    drop(file);

    fs::rename(&temporary, path)?;
    guard.committed = true;

    #[cfg(unix)]
    {
        if let Ok(directory) = fs::File::open(parent) {
            let _ = directory.sync_all();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::atomic_write;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn replaces_an_existing_file_and_leaves_no_temporary_sibling() {
        let directory = tempdir().expect("temporary directory");
        let target = directory.path().join("note.md");
        fs::write(&target, "old").expect("seed target");

        atomic_write(&target, b"new body").expect("replace target");

        assert_eq!(fs::read_to_string(&target).unwrap(), "new body");
        let siblings = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(siblings, vec!["note.md"]);
    }

    #[test]
    fn concurrent_writes_never_share_a_temporary_path() {
        let directory = tempdir().expect("temporary directory");
        let target = directory.path().join("note.md");
        fs::write(&target, "seed").expect("seed target");
        let left = target.clone();
        let right = target.clone();

        let first = std::thread::spawn(move || atomic_write(&left, b"first"));
        let second = std::thread::spawn(move || atomic_write(&right, b"second"));
        first.join().unwrap().unwrap();
        second.join().unwrap().unwrap();

        let body = fs::read_to_string(&target).unwrap();
        assert!(body == "first" || body == "second");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn failed_replacement_preserves_existing_directory_contents_and_cleans_temporary_file() {
        let directory = tempdir().expect("temporary directory");
        let target = directory.path().join("note.md");
        fs::create_dir(&target).expect("seed target directory");
        fs::write(target.join("old.md"), "old body").expect("seed old content");

        assert!(atomic_write(&target, b"new body").is_err());

        assert_eq!(
            fs::read_to_string(target.join("old.md")).expect("preserved old content"),
            "old body"
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
