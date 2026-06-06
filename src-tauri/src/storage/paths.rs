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
    !normalized.starts_with(".iris/")
        && !normalized.starts_with(".classified/")
        && normalized != ".iris"
        && normalized != ".classified"
}

/// Vault-relative path to a note under `.classified/` (not the directory root).
pub fn is_classified_note_path(relative: &str) -> bool {
    let normalized = relative.replace('\\', "/");
    normalized.starts_with(".classified/") && normalized != ".classified"
}

/// Readable/writable note path: ordinary user notes or classified vault notes.
pub fn is_accessible_note_path(relative: &str) -> bool {
    is_user_note_path(relative) || is_classified_note_path(relative)
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

/// Combined check used by AI tool handlers:
/// 1) reject `.iris/` metadata paths
/// 2) reject path traversal (../, absolute, etc.)
/// 3) resolve within vault
pub fn validate_user_note_relative_path(vault: &Path, relative: &str) -> AppResult<PathBuf> {
    if !is_user_note_path(relative) {
        return Err(AppError::msg("只能读取用户笔记，不允许访问内部元数据路径"));
    }
    resolve_vault_path(vault, relative)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    // ── resolve_vault_path ───────────────────────────────

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
    fn rejects_path_with_embedded_parent_dir() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(vault.join("sub")).unwrap();
        let err = resolve_vault_path(&vault, "sub/../../etc/passwd").unwrap_err();
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

    // ── is_user_note_path ────────────────────────────────

    #[test]
    fn is_user_note_path_rejects_iris_metadata() {
        assert!(!is_user_note_path(".iris/versions/1/20260101120000.md"));
        assert!(!is_user_note_path(".iris/skills/some-skill/SKILL.md"));
        assert!(!is_user_note_path(".iris/skills-config.json"));
        assert!(is_user_note_path("notes/readme.md"));
    }

    #[test]
    fn is_user_note_path_rejects_bare_iris() {
        assert!(!is_user_note_path(".iris"));
    }

    #[test]
    fn is_user_note_path_allows_normal_paths() {
        assert!(is_user_note_path("readme.md"));
        assert!(is_user_note_path("docs/guide.md"));
        assert!(is_user_note_path("2024/01/note.md"));
        assert!(is_user_note_path("i.ris/notes.md"));
    }

    #[test]
    fn is_accessible_note_path_includes_classified_notes() {
        assert!(is_accessible_note_path(".classified/secret.md"));
        assert!(!is_accessible_note_path(".classified"));
        assert!(is_accessible_note_path("notes/open.md"));
        assert!(!is_accessible_note_path(".iris/meta.md"));
    }

    #[test]
    fn rejects_classified_dir_and_children() {
        assert!(!is_user_note_path(".classified"));
        assert!(!is_user_note_path(".classified/secret.md"));
        assert!(!is_user_note_path(".classified/sub/dir/file.md"));
        // Windows 反斜杠路径应经 normalize 后同样拒绝
        assert!(!is_user_note_path(".classified\\secret.md"));
        assert!(!is_user_note_path(".classified\\sub\\dir\\file.md"));
    }

    #[test]
    fn still_accepts_normal_paths() {
        assert!(is_user_note_path("notes/readme.md"));
        assert!(is_user_note_path("projects/plan.md"));
        assert!(is_user_note_path("   leading spaces.md"));
        assert!(is_user_note_path("notes\\readme.md"));
        assert!(is_user_note_path("projects\\plan.md"));
    }

    // ── validate_user_note_relative_path (combined) ──────

    #[test]
    fn validate_rejects_iris_metadata_dir() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let err = validate_user_note_relative_path(&vault, ".iris/versions/1/test.md").unwrap_err();
        assert!(err.to_string().contains("内部元数据"));
    }

    #[test]
    fn validate_rejects_parent_dir_traversal() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let err = validate_user_note_relative_path(&vault, "../secret.md").unwrap_err();
        assert!(err.to_string().contains("traversal"));
    }

    #[test]
    fn validate_rejects_absolute_path() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let err = validate_user_note_relative_path(&vault, "/etc/passwd").unwrap_err();
        assert!(err.to_string().contains("traversal"));
    }

    #[test]
    fn validate_rejects_embedded_parent_in_middle() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(vault.join("sub")).unwrap();
        let err = validate_user_note_relative_path(&vault, "sub/../../etc/passwd").unwrap_err();
        assert!(err.to_string().contains("traversal"));
    }

    #[test]
    fn validate_accepts_valid_user_note() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let note = vault.join("notes/readme.md");
        fs::create_dir_all(note.parent().unwrap()).unwrap();
        fs::write(&note, "# Hello").unwrap();
        let resolved = validate_user_note_relative_path(&vault, "notes/readme.md").unwrap();
        assert_eq!(resolved, note.canonicalize().unwrap());
    }

    // ── relative_path ────────────────────────────────────

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
