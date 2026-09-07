//! Create-only binary resources shared by editor and authorized tool adapters.
//! No note versions, model parameters or indexing belong to this boundary.

use std::path::Path;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::storage::atomic_write::{atomic_create, with_vault_move_lock};
use crate::storage::paths::{ensure_safe_file_parent, resolve_vault_path};

const MAX_ASSET_BYTES: usize = 20 * 1024 * 1024;

/// Resource names are exact forward-slash paths below the public assets root.
pub(crate) fn is_vault_asset_path(relative: &str) -> bool {
    relative.starts_with("assets/")
        && !relative.contains('\\')
        && relative
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// Reject oversized encoded input before allocating its decoded buffer.
pub(crate) fn decode_asset(data: &str) -> AppResult<Vec<u8>> {
    let data = data.trim();
    if data.len() > MAX_ASSET_BYTES.div_ceil(3) * 4 {
        return Err(AppError::msg("资源超过 20MB 限制"));
    }
    let bytes = STANDARD
        .decode(data)
        .map_err(|_| AppError::msg("无效的资源数据"))?;
    validate_bytes(&bytes)?;
    Ok(bytes)
}

fn validate_bytes(bytes: &[u8]) -> AppResult<()> {
    if bytes.is_empty() {
        return Err(AppError::msg("资源数据为空"));
    }
    if bytes.len() > MAX_ASSET_BYTES {
        return Err(AppError::msg("资源超过 20MB 限制"));
    }
    Ok(())
}

/// Validate scope and the captured vault before any mkdir or no-clobber publish.
pub(crate) fn create_asset(
    state: &AppState,
    expected_vault: &Path,
    path: &str,
    bytes: &[u8],
    authorize: impl FnOnce() -> AppResult<()>,
) -> AppResult<()> {
    if !is_vault_asset_path(path) {
        return Err(AppError::msg("资源路径必须位于 assets/ 下"));
    }
    validate_bytes(bytes)?;
    let expected_vault = expected_vault
        .canonicalize()
        .map_err(|_| AppError::msg("note_vault_changed"))?;
    with_vault_move_lock(|| {
        if state.vault_path()? != expected_vault {
            return Err(AppError::msg("note_vault_changed"));
        }
        authorize()?;
        ensure_safe_file_parent(&expected_vault, path)?;
        let absolute = resolve_vault_path(&expected_vault, path)?;
        atomic_create(&absolute, bytes)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> (std::sync::Arc<AppState>, tempfile::TempDir) {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir(&vault).unwrap();
        let state = AppState::new(temp.path().join("data")).unwrap();
        state.set_vault(vault).unwrap();
        (state, temp)
    }

    #[test]
    fn creates_exact_bytes_and_never_overwrites() {
        let (state, _temp) = state();
        let vault = state.vault_path().unwrap();
        create_asset(
            &state,
            &vault,
            "assets/nested/image.png",
            b"original",
            || Ok(()),
        )
        .unwrap();
        assert!(
            create_asset(&state, &vault, "assets/nested/image.png", b"new", || Ok(())).is_err()
        );
        assert_eq!(
            std::fs::read(vault.join("assets/nested/image.png")).unwrap(),
            b"original"
        );
    }

    #[test]
    fn rejected_scope_and_invalid_bytes_leave_no_directories() {
        let (state, _temp) = state();
        let vault = state.vault_path().unwrap();
        assert!(
            create_asset(&state, &vault, "assets/nested/image.png", b"body", || {
                Err(AppError::msg("scope_denied"))
            })
            .is_err()
        );
        assert!(create_asset(&state, &vault, "assets/nested/image.png", b"", || Ok(())).is_err());
        assert!(!vault.join("assets").exists());
    }

    #[test]
    fn queued_asset_cannot_follow_a_vault_switch() {
        let (state, temp) = state();
        let original = state.vault_path().unwrap();
        let other = temp.path().join("other");
        std::fs::create_dir(&other).unwrap();
        state.set_vault(other.clone()).unwrap();
        let error =
            create_asset(&state, &original, "assets/image.png", b"body", || Ok(())).unwrap_err();
        assert!(error.to_string().contains("note_vault_changed"));
        assert!(!original.join("assets").exists());
        assert!(!other.join("assets").exists());
    }

    #[test]
    fn encoded_payload_validation_is_bounded() {
        assert_eq!(decode_asset(" Ym9keQ== ").unwrap(), b"body");
        assert!(decode_asset("").is_err());
        assert!(decode_asset("not encoded").is_err());
        assert!(decode_asset(&"A".repeat(MAX_ASSET_BYTES.div_ceil(3) * 4 + 1)).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn asset_alias_cannot_enter_internal_metadata() {
        let (state, _temp) = state();
        let vault = state.vault_path().unwrap();
        std::fs::create_dir_all(vault.join(".iris/private")).unwrap();
        std::os::unix::fs::symlink(vault.join(".iris/private"), vault.join("assets")).unwrap();
        assert!(create_asset(
            &state,
            &vault,
            "assets/nested/image.png",
            b"body",
            || Ok(())
        )
        .is_err());
        assert!(!vault.join(".iris/private/nested").exists());
    }
}
