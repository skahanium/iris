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
    let (temporary, mut guard) = write_synced_temporary(parent, path, data)?;

    fs::rename(&temporary, path)?;
    guard.committed = true;
    sync_parent_directory(parent)?;

    Ok(())
}

/// Durably create a new file without replacing an existing target.
///
/// The final hard-link operation is an atomic no-clobber publish on the same
/// filesystem as the unique, synced sibling temporary file. A target created
/// by a concurrent caller therefore wins without exposing a partial body.
pub(crate) fn atomic_create(path: &Path, data: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::msg("atomic create target has no parent"))?;
    fs::create_dir_all(parent)?;
    let (temporary, _guard) = write_synced_temporary(parent, path, data)?;

    fs::hard_link(&temporary, path)?;
    fs::remove_file(&temporary)?;
    sync_parent_directory(parent)?;

    Ok(())
}

fn write_synced_temporary(
    parent: &Path,
    path: &Path,
    data: &[u8],
) -> AppResult<(PathBuf, TemporaryFileGuard)> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::msg("atomic write target has an invalid file name"))?;
    let temporary = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
    let guard = TemporaryFileGuard::new(temporary.clone());

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(data)?;
    file.sync_all()?;
    drop(file);

    Ok((temporary, guard))
}

fn sync_parent_directory(parent: &Path) -> AppResult<()> {
    #[cfg(unix)]
    {
        let directory = fs::File::open(parent)?;
        directory.sync_all()?;
    }

    #[cfg(not(unix))]
    let _ = parent;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{atomic_create, atomic_write, sync_parent_directory};
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

    #[test]
    fn atomic_create_never_replaces_an_existing_file() {
        let directory = tempdir().expect("temporary directory");
        let target = directory.path().join("note.md");
        fs::write(&target, "existing body").expect("seed target");

        let error = atomic_create(&target, b"new body").expect_err("existing target must win");

        assert!(matches!(
            error,
            crate::error::AppError::Io(ref io_error)
                if io_error.kind() == std::io::ErrorKind::AlreadyExists
        ));
        assert_eq!(fs::read_to_string(&target).unwrap(), "existing body");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn concurrent_atomic_creates_have_exactly_one_winner() {
        let directory = tempdir().expect("temporary directory");
        let target = directory.path().join("note.md");
        let left = target.clone();
        let right = target.clone();

        let first = std::thread::spawn(move || atomic_create(&left, b"first"));
        let second = std::thread::spawn(move || atomic_create(&right, b"second"));
        let outcomes: [crate::error::AppResult<()>; 2] =
            [first.join().unwrap(), second.join().unwrap()];

        assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
        assert!(outcomes.iter().any(|result| matches!(
            result,
            Err(crate::error::AppError::Io(error))
                if error.kind() == std::io::ErrorKind::AlreadyExists
        )));
        let body = fs::read_to_string(&target).unwrap();
        assert!(body == "first" || body == "second");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn parent_directory_sync_errors_are_not_silently_ignored() {
        let directory = tempdir().expect("temporary directory");
        let missing = directory.path().join("missing-parent");

        assert!(sync_parent_directory(&missing).is_err());
    }
}
