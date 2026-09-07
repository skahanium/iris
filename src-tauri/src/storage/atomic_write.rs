use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use uuid::Uuid;

use crate::error::{AppError, AppResult};

static VAULT_MOVE_LOCK: Mutex<()> = Mutex::new(());

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

/// Serialize one vault move operation, including its post-move bookkeeping.
///
/// This lock is **not reentrant**. Callers that already hold it must use
/// [`crate::storage::note_write::NoteWriteService::write_under_move_lock`]
/// instead of APIs that acquire this lock again.
pub(crate) fn with_vault_move_lock<T>(operation: impl FnOnce() -> AppResult<T>) -> AppResult<T> {
    let _guard = VAULT_MOVE_LOCK
        .lock()
        .map_err(|_| AppError::msg("vault move coordinator is unavailable"))?;
    operation()
}

/// Move one regular file without replacing an existing destination.
///
/// `hard_link` is an atomic no-replace publish on supported local filesystems
/// on both Windows and Unix. Vault-internal moves stay on one filesystem; if a
/// filesystem lacks this primitive we fail safely and preserve the source.
pub(crate) fn move_file_no_replace_locked(source: &Path, target: &Path) -> AppResult<()> {
    if !fs::symlink_metadata(source)?.file_type().is_file() {
        return Err(AppError::msg(
            "no-replace move source is not a regular file",
        ));
    }
    let target_parent = target
        .parent()
        .ok_or_else(|| AppError::msg("no-replace move target has no parent"))?;
    fs::create_dir_all(target_parent)?;

    fs::hard_link(source, target)?;
    if let Err(error) = sync_parent_directory(target_parent) {
        let _ = fs::remove_file(target);
        return Err(error);
    }
    if let Err(error) = fs::remove_file(source) {
        let _ = fs::remove_file(target);
        return Err(error.into());
    }

    let source_parent = source
        .parent()
        .ok_or_else(|| AppError::msg("no-replace move source has no parent"))?;
    sync_parent_directory(source_parent)?;
    if source_parent != target_parent {
        sync_parent_directory(target_parent)?;
    }
    Ok(())
}

/// Publish a second name for a regular file without replacing an existing
/// target. The source is retained so a durable journal can record the linked
/// intermediate state before removing it.
pub(crate) fn link_file_no_replace_locked(source: &Path, target: &Path) -> AppResult<()> {
    if !fs::symlink_metadata(source)?.file_type().is_file() {
        return Err(AppError::msg(
            "no-replace link source is not a regular file",
        ));
    }
    let target_parent = target
        .parent()
        .ok_or_else(|| AppError::msg("no-replace link target has no parent"))?;
    fs::create_dir_all(target_parent)?;
    fs::hard_link(source, target)?;
    if let Err(error) = sync_parent_directory(target_parent) {
        let _ = fs::remove_file(target);
        return Err(error);
    }
    Ok(())
}

/// Durably remove one file after its replacement name has been checkpointed.
pub(crate) fn remove_file_durable(path: &Path) -> AppResult<()> {
    fs::remove_file(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| AppError::msg("durable remove target has no parent"))?;
    sync_parent_directory(parent)
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

pub(crate) fn sync_parent_directory(parent: &Path) -> AppResult<()> {
    #[cfg(unix)]
    {
        let directory = fs::File::open(parent)?;
        directory.sync_all()?;
    }

    #[cfg(not(unix))]
    {
        // Rust exposes no portable directory fsync on Windows. We still must
        // validate that the parent exists and is a directory, so a failed
        // durability check is never acknowledged as a successful no-op.
        if !fs::metadata(parent)?.is_dir() {
            return Err(AppError::msg("atomic write parent is not a directory"));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{atomic_create, atomic_write, move_file_no_replace_locked, sync_parent_directory};
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

    #[test]
    fn concurrent_no_replace_moves_have_one_winner_without_overwriting_the_target() {
        let directory = tempdir().expect("temporary directory");
        let first_source = directory.path().join("first.md");
        let second_source = directory.path().join("second.md");
        let target = directory.path().join("target.md");
        fs::write(&first_source, "first body").unwrap();
        fs::write(&second_source, "second body").unwrap();

        let first_target = target.clone();
        let second_target = target.clone();
        let first =
            std::thread::spawn(move || move_file_no_replace_locked(&first_source, &first_target));
        let second =
            std::thread::spawn(move || move_file_no_replace_locked(&second_source, &second_target));
        let outcomes: [crate::error::AppResult<()>; 2] =
            [first.join().unwrap(), second.join().unwrap()];

        assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
        assert!(outcomes.iter().any(|result| matches!(
            result,
            Err(crate::error::AppError::Io(error))
                if error.kind() == std::io::ErrorKind::AlreadyExists
        )));
        let target_body = fs::read_to_string(&target).unwrap();
        assert!(target_body == "first body" || target_body == "second body");
        assert_eq!(
            [
                directory.path().join("first.md"),
                directory.path().join("second.md")
            ]
            .iter()
            .filter(|path| path.is_file())
            .count(),
            1
        );
    }
}
