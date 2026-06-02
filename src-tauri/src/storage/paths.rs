use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, AppResult};

/// Resolve a relative path under the vault and reject path traversal.
pub fn resolve_vault_path(vault: &Path, relative: &str) -> AppResult<PathBuf> {
    let vault = vault
        .canonicalize()
        .map_err(|e| AppError::msg(format!("Invalid vault path: {e}")))?;

    let mut joined = vault.clone();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => joined.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::msg("Path traversal is not allowed"));
            }
        }
    }

    // For new files that don't exist yet, canonicalize would fail.
    // Canonicalize the parent (which must exist) and append the filename.
    if joined.exists() {
        let canonical = joined
            .canonicalize()
            .map_err(|_| AppError::msg("Path is outside the vault"))?;
        if !canonical.starts_with(&vault) {
            return Err(AppError::msg("Path is outside the vault"));
        }
        Ok(canonical)
    } else {
        let parent = joined
            .parent()
            .ok_or_else(|| AppError::msg("Invalid path"))?;
        let file_name = joined
            .file_name()
            .ok_or_else(|| AppError::msg("Invalid path"))?;
        let canonical_parent = parent
            .canonicalize()
            .map_err(|_| AppError::msg("Path is outside the vault"))?;
        if !canonical_parent.starts_with(&vault) {
            return Err(AppError::msg("Path is outside the vault"));
        }
        Ok(canonical_parent.join(file_name))
    }
}

/// 用户笔记（非 `.iris` 元数据目录下的版本快照、模板等）。
pub fn is_user_note_path(relative: &str) -> bool {
    let normalized = relative.replace('\\', "/");
    !normalized.starts_with(".iris/") && normalized != ".iris"
}

/// Read file content as UTF-8, replacing invalid bytes with U+FFFD.
/// Prevents crash on non-UTF-8 encoded files. Logs a warning if replacements occurred.
pub fn read_file_lossy(path: &std::path::Path) -> AppResult<String> {
    let bytes = std::fs::read(path)?;
    let content = String::from_utf8_lossy(&bytes);
    let has_replacements = content.contains('\u{FFFD}');
    if has_replacements {
        tracing::warn!(
            path = %path.display(),
            "file contains non-UTF-8 bytes; replaced with U+FFFD"
        );
    }
    Ok(content.into_owned())
}

/// Relative path from vault root (forward slashes).
pub fn relative_path(vault: &Path, absolute: &Path) -> AppResult<String> {
    let vault = vault.canonicalize()?;
    let absolute = absolute.canonicalize()?;
    let rel = absolute
        .strip_prefix(&vault)
        .map_err(|_| AppError::msg("Path is outside the vault"))?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn rejects_parent_dir() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let err = resolve_vault_path(&vault, "../secret").unwrap_err();
        assert!(err.to_string().contains("traversal"));
    }

    #[test]
    fn rejects_absolute_path() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let err = resolve_vault_path(&vault, "/etc/passwd").unwrap_err();
        assert!(err.to_string().contains("traversal"));
    }

    #[test]
    fn resolves_valid_subdirectory_file() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        let sub = vault.join("sub");
        fs::create_dir_all(&sub).unwrap();
        let note = sub.join("note.md");
        fs::write(&note, "hello").unwrap();
        let resolved = resolve_vault_path(&vault, "sub/note.md").unwrap();
        assert_eq!(resolved, note.canonicalize().unwrap());
    }

    #[test]
    fn is_user_note_path_rejects_iris_metadata() {
        assert!(!is_user_note_path(".iris/versions/1/20260101120000.md"));
        assert!(is_user_note_path("notes/readme.md"));
    }

    #[test]
    fn relative_path_normal_case() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let note = vault.join("notes/readme.md");
        fs::create_dir_all(note.parent().unwrap()).unwrap();
        fs::write(&note, "").unwrap();
        let rel = relative_path(&vault, &note).unwrap();
        assert_eq!(rel, "notes/readme.md");
    }
}
