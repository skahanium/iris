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

    let canonical = joined
        .canonicalize()
        .map_err(|_| AppError::msg("Path is outside the vault"))?;

    if !canonical.starts_with(&vault) {
        return Err(AppError::msg("Path is outside the vault"));
    }

    Ok(canonical)
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
}
