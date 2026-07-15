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

/// Move a directory tree without replacing an existing destination.
///
/// `std` has no cross-platform directory equivalent of no-replace rename. We
/// reserve the final directory atomically with `create_dir`, move regular
/// files through the no-replace primitive, and compensate every completed
/// child move if any later step fails. Callers hold [`with_vault_move_lock`]
/// for the complete higher-level operation.
pub(crate) fn move_directory_no_replace_locked(source: &Path, target: &Path) -> AppResult<()> {
    move_directory_no_replace_with_sync(source, target, sync_parent_directory)
}

fn move_directory_no_replace_with_sync<F>(
    source: &Path,
    target: &Path,
    sync_parent: F,
) -> AppResult<()>
where
    F: Fn(&Path) -> AppResult<()>,
{
    if !fs::symlink_metadata(source)?.file_type().is_dir() {
        return Err(AppError::msg("no-replace move source is not a directory"));
    }
    if target.starts_with(source) {
        return Err(AppError::msg("cannot move a directory into itself"));
    }
    let target_parent = target
        .parent()
        .ok_or_else(|| AppError::msg("no-replace move target has no parent"))?;
    fs::create_dir_all(target_parent)?;
    fs::create_dir(target)?;

    let mut moved_files = Vec::new();
    let mut created_directories = Vec::new();
    let result =
        move_directory_contents(source, target, &mut moved_files, &mut created_directories);
    if result.is_err() {
        return rollback_directory_move_or_report(
            &moved_files,
            &created_directories,
            source,
            target,
            &sync_parent,
            "directory move failed; source restored",
            "directory_move_rollback_completed",
        );
    }

    if fs::remove_dir(source).is_err() {
        return rollback_directory_move_or_report(
            &moved_files,
            &created_directories,
            source,
            target,
            &sync_parent,
            "directory move failed; source restored",
            "directory_move_rollback_completed",
        );
    }
    let source_parent = source
        .parent()
        .ok_or_else(|| AppError::msg("no-replace move source has no parent"))?;
    if sync_parent(source_parent).is_err() {
        return rollback_directory_move_or_report(
            &moved_files,
            &created_directories,
            source,
            target,
            &sync_parent,
            "directory move durability check failed; source restored",
            "directory_move_durability_rollback_completed",
        );
    }
    if source_parent != target_parent && sync_parent(target_parent).is_err() {
        return rollback_directory_move_or_report(
            &moved_files,
            &created_directories,
            source,
            target,
            &sync_parent,
            "directory move durability check failed; source restored",
            "directory_move_durability_rollback_completed",
        );
    }
    Ok(())
}

fn move_directory_contents(
    source: &Path,
    target: &Path,
    moved_files: &mut Vec<(PathBuf, PathBuf)>,
    created_directories: &mut Vec<PathBuf>,
) -> AppResult<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_child = entry.path();
        let target_child = target.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_child)?;
        if metadata.is_file() {
            move_file_no_replace_locked(&source_child, &target_child)?;
            moved_files.push((source_child, target_child));
        } else if metadata.is_dir() {
            fs::create_dir(&target_child)?;
            created_directories.push(target_child.clone());
            move_directory_contents(
                &source_child,
                &target_child,
                moved_files,
                created_directories,
            )?;
            fs::remove_dir(&source_child)?;
        } else {
            return Err(AppError::msg(
                "no-replace directory move does not support special filesystem entries",
            ));
        }
    }
    Ok(())
}

fn rollback_directory_move_or_report<F>(
    moved_files: &[(PathBuf, PathBuf)],
    created_directories: &[PathBuf],
    source: &Path,
    target: &Path,
    sync_parent: &F,
    restored_message: &'static str,
    restored_result_code: &'static str,
) -> AppResult<()>
where
    F: Fn(&Path) -> AppResult<()>,
{
    if rollback_directory_move(
        moved_files,
        created_directories,
        source,
        target,
        sync_parent,
    )
    .is_ok()
    {
        tracing::warn!(
            result_code = restored_result_code,
            "directory move failed and its source tree was restored"
        );
        return Err(AppError::msg(restored_message));
    }
    tracing::error!(
        result_code = "directory_move_rollback_incomplete",
        "directory move failed and its source tree could not be fully restored"
    );
    Err(AppError::msg(
        "directory move failed; rollback could not be completed",
    ))
}

fn rollback_directory_move<F>(
    moved_files: &[(PathBuf, PathBuf)],
    created_directories: &[PathBuf],
    source: &Path,
    target: &Path,
    sync_parent: &F,
) -> AppResult<()>
where
    F: Fn(&Path) -> AppResult<()>,
{
    let mut rollback_error = None;
    if !source.exists() {
        if let Err(error) = fs::create_dir(source) {
            rollback_error = Some(error.into());
        }
    }
    for (source, target) in moved_files.iter().rev() {
        if let Err(error) = move_file_no_replace_locked(target, source) {
            rollback_error.get_or_insert(error);
        }
    }
    for directory in created_directories.iter().rev() {
        if let Err(error) = fs::remove_dir(directory) {
            rollback_error.get_or_insert(error.into());
        }
    }
    if let Err(error) = fs::remove_dir(target) {
        rollback_error.get_or_insert(error.into());
    }

    let source_parent = source
        .parent()
        .ok_or_else(|| AppError::msg("no-replace move source has no parent"))?;
    if let Err(error) = sync_parent(source_parent) {
        rollback_error.get_or_insert(error);
    }
    let target_parent = target
        .parent()
        .ok_or_else(|| AppError::msg("no-replace move target has no parent"))?;
    if source_parent != target_parent {
        match sync_parent(target_parent) {
            Ok(()) => {}
            Err(error) => {
                rollback_error.get_or_insert(error);
            }
        }
    }

    rollback_error.map_or(Ok(()), Err)
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
    use super::{
        atomic_create, atomic_write, move_directory_no_replace_locked,
        move_directory_no_replace_with_sync, move_file_no_replace_locked, sync_parent_directory,
    };
    use std::cell::Cell;
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

    #[test]
    fn no_replace_directory_move_keeps_source_when_target_is_occupied() {
        let directory = tempdir().expect("temporary directory");
        let source = directory.path().join("source");
        let target = directory.path().join("target");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(source.join("note.md"), "source body").unwrap();
        fs::write(target.join("note.md"), "target body").unwrap();

        assert!(move_directory_no_replace_locked(&source, &target).is_err());

        assert_eq!(
            fs::read_to_string(source.join("note.md")).unwrap(),
            "source body"
        );
        assert_eq!(
            fs::read_to_string(target.join("note.md")).unwrap(),
            "target body"
        );
    }

    #[test]
    fn directory_move_restores_source_when_final_parent_sync_fails() {
        let directory = tempdir().expect("temporary directory");
        let source = directory.path().join("source");
        let target = directory.path().join("target");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("nested/note.md"), "source body").unwrap();
        let source_parent = source.parent().unwrap().to_path_buf();
        let failed_once = Cell::new(false);

        let error = move_directory_no_replace_with_sync(&source, &target, |parent| {
            if parent == source_parent && !failed_once.replace(true) {
                return Err(crate::error::AppError::Io(std::io::Error::other(
                    "injected final directory sync failure",
                )));
            }
            sync_parent_directory(parent)
        })
        .expect_err("a failed final sync must not acknowledge the move");

        assert!(error.to_string().contains("source restored"));
        assert_eq!(
            fs::read_to_string(source.join("nested/note.md")).unwrap(),
            "source body"
        );
        assert!(!target.exists());
    }

    #[test]
    fn directory_move_reports_when_rollback_durability_cannot_be_confirmed() {
        let directory = tempdir().expect("temporary directory");
        let source = directory.path().join("source");
        let target = directory.path().join("target");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("note.md"), "source body").unwrap();

        let error = move_directory_no_replace_with_sync(&source, &target, |_| {
            Err(crate::error::AppError::Io(std::io::Error::other(
                "injected persistent directory sync failure",
            )))
        })
        .expect_err("an unconfirmed rollback must not be reported as restored");

        assert!(error
            .to_string()
            .contains("rollback could not be completed"));
        // The best-effort compensation still restores the visible namespace,
        // but the returned error preserves that its durability was not proven.
        assert_eq!(
            fs::read_to_string(source.join("note.md")).unwrap(),
            "source body"
        );
        assert!(!target.exists());
    }
}
