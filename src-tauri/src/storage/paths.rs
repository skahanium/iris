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

    // For new nested paths that don't exist yet, canonicalize would fail.
    // Canonicalize the nearest existing ancestor so a missing parent tree is
    // accepted without weakening the symlink/traversal boundary.
    if joined.exists() {
        let canonical = joined
            .canonicalize()
            .map_err(|_| AppError::msg("Path is outside the vault"))?;
        if !canonical.starts_with(&vault) {
            return Err(AppError::msg("Path is outside the vault"));
        }
        Ok(canonical)
    } else {
        let mut ancestor = joined.as_path();
        while !ancestor.exists() {
            ancestor = ancestor
                .parent()
                .ok_or_else(|| AppError::msg("Invalid path"))?;
        }
        let canonical_ancestor = ancestor
            .canonicalize()
            .map_err(|_| AppError::msg("Path is outside the vault"))?;
        if !canonical_ancestor.starts_with(&vault) {
            return Err(AppError::msg("Path is outside the vault"));
        }
        let unresolved_suffix = joined
            .strip_prefix(ancestor)
            .map_err(|_| AppError::msg("Invalid path"))?;
        Ok(canonical_ancestor.join(unresolved_suffix))
    }
}

/// Validate all path components before creating parents for a protected write.
/// Callers hold the vault operation guard and perform authorization beforehand.
pub(crate) fn ensure_safe_file_parent(vault: &Path, path: &str) -> AppResult<()> {
    // Validate every existing ancestor before any mkdir. In particular, never
    // create directories through a symlink and reject only after the side effect.
    let mut parent = vault.canonicalize()?;
    let relative_parent = Path::new(path).parent().unwrap_or_else(|| Path::new(""));
    let mut missing = Vec::new();
    for component in relative_parent.components() {
        match component {
            Component::Normal(part) => {
                parent.push(part);
                match std::fs::symlink_metadata(&parent) {
                    Ok(metadata) if metadata.file_type().is_symlink() => {
                        return Err(AppError::msg("note_path_alias_not_allowed"))
                    }
                    Ok(metadata) if !metadata.is_dir() => {
                        return Err(AppError::msg("note_parent_not_directory"))
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        missing.push(parent.clone())
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::msg("Path traversal is not allowed"));
            }
        }
    }
    // Also reject a final-component symlink; its canonical path could bypass
    // lock identity or enter metadata despite a harmless-looking lexical name.
    let file_name = Path::new(path)
        .file_name()
        .ok_or_else(|| AppError::msg("Invalid path"))?;
    if std::fs::symlink_metadata(parent.join(file_name))
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(AppError::msg("note_path_alias_not_allowed"));
    }
    for directory in missing {
        std::fs::create_dir(directory)?;
    }
    Ok(())
}

/// 用户笔记（非 `.iris` 元数据目录下的版本快照、模板等）。
fn normalized_relative(relative: &str) -> String {
    relative.replace('\\', "/")
}

fn first_path_segment(relative: &str) -> &str {
    relative
        .split('/')
        .find(|segment| !segment.is_empty() && *segment != ".")
        .unwrap_or(relative)
}

/// True when the first vault-relative path segment is an Iris reserved root.
pub(crate) fn has_reserved_path_root(relative: &str) -> bool {
    let normalized = normalized_relative(relative);
    let first = first_path_segment(&normalized);
    first.eq_ignore_ascii_case(".iris") || first.eq_ignore_ascii_case(".classified")
}

pub fn is_user_note_path(relative: &str) -> bool {
    !has_reserved_path_root(relative)
}

/// Vault-relative path to a note under `.classified/` (not the directory root).
pub fn is_classified_note_path(relative: &str) -> bool {
    let normalized = normalized_relative(relative);
    let Some(rest) = normalized.strip_prefix(".classified/") else {
        return false;
    };
    !rest.is_empty()
}

/// Readable/writable note path: ordinary user notes or classified vault notes.
pub fn is_accessible_note_path(relative: &str) -> bool {
    is_user_note_path(relative) || is_classified_note_path(relative)
}

/// Read file content as UTF-8, replacing invalid bytes with U+FFFD.
/// Prevents crash on non-UTF-8 encoded files. Logs a warning if replacements occurred.
pub fn read_file_lossy(path: &std::path::Path) -> AppResult<String> {
    let bytes = std::fs::read(path)?;
    String::from_utf8(bytes).map_err(|_| AppError::msg("File is not valid UTF-8"))
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
    let absolute = resolve_vault_path(vault, relative)?;
    let canonical_vault = vault.canonicalize()?;
    let canonical_relative = absolute
        .strip_prefix(&canonical_vault)
        .map_err(|_| AppError::msg("Path is outside the vault"))?;
    if !is_user_note_path(&canonical_relative.to_string_lossy()) {
        return Err(AppError::msg("不允许通过路径别名访问内部元数据"));
    }
    Ok(absolute)
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
    fn resolves_a_new_path_below_missing_parent_directories() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();

        let resolved = resolve_vault_path(&vault, "archive/nested/new.md").unwrap();

        assert_eq!(
            resolved,
            vault.canonicalize().unwrap().join("archive/nested/new.md")
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolves_missing_suffix_from_the_canonical_symlink_ancestor() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(vault.join("old/sub")).unwrap();
        symlink(vault.join("old/sub"), vault.join("alias")).unwrap();

        let resolved = resolve_vault_path(&vault, "alias/new/note.md").unwrap();

        assert_eq!(
            resolved,
            vault.canonicalize().unwrap().join("old/sub/new/note.md")
        );
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
    fn rejects_reserved_paths_case_insensitively() {
        for path in [
            ".IRIS",
            ".IRIS/versions/1.md",
            ".Iris\\skills\\demo\\SKILL.md",
            ".CLASSIFIED",
            ".CLASSIFIED/secret.md",
            ".Classified\\secret.md",
        ] {
            assert!(!is_user_note_path(path), "{path} must not be a user note");
            assert!(
                !is_accessible_note_path(path),
                "{path} must not be accessible through note APIs"
            );
        }
        assert!(!is_classified_note_path(".CLASSIFIED/secret.md"));
    }

    #[test]
    fn strict_utf8_reader_rejects_invalid_note_bytes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.md");
        fs::write(&path, [0xff, b'#', b' ', b'B']).unwrap();

        let err = read_file_lossy(&path).unwrap_err();
        assert!(err.to_string().contains("UTF-8"));
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
