//! Secure file deletion helpers for sensitive temporary files.

use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

#[cfg(test)]
use std::path::PathBuf;

#[cfg(test)]
use crate::error::AppError;
use crate::error::AppResult;

/// Return the Iris-owned temp directory used by tests and secure temp helpers.
#[cfg(test)]
pub fn user_temp_dir() -> PathBuf {
    if let Some(temp) = std::env::var_os("IRIS_TEMP_DIR") {
        let path = PathBuf::from(temp);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    if let Some(home) = std::env::var_os("IRIS_HOME") {
        let path = PathBuf::from(home);
        if !path.as_os_str().is_empty() {
            return path.join("tmp");
        }
    }
    std::env::temp_dir().join("iris-tmp")
}

/// Overwrite a file with zero bytes and then remove it.
pub fn secure_delete(path: &Path) -> AppResult<()> {
    if !path.exists() {
        return Ok(());
    }

    let metadata = std::fs::metadata(path)?;
    let len = metadata.len();

    let mut file = OpenOptions::new().write(true).open(path)?;
    file.seek(SeekFrom::Start(0))?;

    let zeros = vec![0u8; 4096];
    let mut written = 0u64;
    while written < len {
        let chunk = std::cmp::min(4096, (len - written) as usize);
        file.write_all(&zeros[..chunk])?;
        written += chunk as u64;
    }
    file.sync_all()?;

    std::fs::remove_file(path)?;

    Ok(())
}

/// Recursively overwrite files in a directory before removing the tree.
#[cfg(test)]
pub fn secure_remove_dir_all(path: &Path) -> AppResult<()> {
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let entry_path = entry.path();
            if entry_path.is_dir() {
                secure_remove_dir_all(&entry_path)?;
            } else {
                let _ = secure_delete(&entry_path);
            }
        }
    }
    std::fs::remove_dir_all(path)
        .map_err(|e| AppError::msg(format!("failed to remove temporary directory: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn secure_delete_removes_file() {
        let tmp = NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "sensitive data").unwrap();
        assert!(tmp.path().exists());

        secure_delete(tmp.path()).unwrap();
        assert!(!tmp.path().exists());
    }

    #[test]
    fn secure_delete_nonexistent_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let ghost = dir.path().join("does_not_exist.txt");
        assert!(secure_delete(&ghost).is_ok());
    }

    #[test]
    fn secure_remove_dir_all_handles_nested_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let sub = tmp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("a.txt"), "hello").unwrap();
        std::fs::create_dir(sub.join("nested")).unwrap();
        std::fs::write(sub.join("nested").join("b.txt"), "world").unwrap();

        assert!(sub.exists());
        secure_remove_dir_all(&sub).unwrap();
        assert!(!sub.exists());
    }

    #[test]
    fn secure_remove_dir_all_nonexistent_is_noop() {
        let ghost = std::path::Path::new("/nonexistent/iris/test/dir");
        assert!(secure_remove_dir_all(ghost).is_ok());
    }

    #[test]
    fn user_temp_dir_returns_a_path() {
        let p = user_temp_dir();
        assert!(!p.as_os_str().is_empty());
    }
}
