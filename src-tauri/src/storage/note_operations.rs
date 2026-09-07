//! Note operations own preconditions, recovery snapshots and persistence,
//! independently of the interaction that proposed the change.
use crate::app::AppState;
use crate::cas::hash::content_hash_str;
use crate::error::{AppError, AppResult};
use crate::storage::atomic_write::with_vault_move_lock;
use crate::storage::note_write::{FileWriteResult, NoteWriteService};
use crate::storage::paths::validate_user_note_relative_path;
use serde::{Deserialize, Serialize};
use std::ops::Range;
use std::path::Path;

/// Exact, content-addressed edit without model or Run parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NoteEdit {
    pub path: String,
    pub base_content_hash: String,
    pub range: Range<usize>,
    pub original: String,
    pub replacement: String,
}

/// Durable body fact and independent recovery/index state.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NoteEditReceipt {
    pub path: String,
    pub before_hash: String,
    pub after_hash: String,
    pub version_id: i64,
    pub write: FileWriteResult,
}

/// Validate an exact range without normalizing unrelated Markdown bytes.
pub(crate) fn edited_content(edit: &NoteEdit, current: &str) -> AppResult<String> {
    if content_hash_str(current) != edit.base_content_hash {
        return Err(AppError::msg("note_content_conflict"));
    }
    if current.get(edit.range.clone()) != Some(edit.original.as_str()) {
        return Err(AppError::msg("note_original_conflict"));
    }
    let length = current.len() - edit.original.len() + edit.replacement.len();
    if length > 20 * 1024 * 1024 {
        return Err(AppError::msg("note_content_too_large"));
    }
    let mut content = String::with_capacity(length);
    content.push_str(&current[..edit.range.start]);
    content.push_str(&edit.replacement);
    content.push_str(&current[edit.range.end..]);
    Ok(content)
}

/// Commit inside the same guard as ordinary saves and moves; the adapter
/// rechecks its authorization/cancellation within that guard.
pub(crate) fn apply_edit(
    state: &AppState,
    expected_vault: &Path,
    edit: &NoteEdit,
    validate_authorization: impl FnOnce() -> AppResult<()>,
) -> AppResult<NoteEditReceipt> {
    with_vault_move_lock(|| {
        if state.vault_path()? != expected_vault {
            return Err(AppError::msg("note_vault_changed"));
        }
        validate_authorization()?;
        NoteWriteService::ensure_unlocked(state, &edit.path)?;
        let absolute = validate_user_note_relative_path(expected_vault, &edit.path)?;
        let current = std::fs::read_to_string(absolute)?;
        let updated = edited_content(edit, &current)?;
        let version_id = protect_snapshot(state, &edit.path, &current)?;
        let write = NoteWriteService::write_under_move_lock(state, &edit.path, &updated)?;
        Ok(NoteEditReceipt {
            path: edit.path.clone(),
            before_hash: edit.base_content_hash.clone(),
            after_hash: write.content_hash.clone(),
            version_id,
            write,
        })
    })
}

/// Deduplication can reuse a readable snapshot; failure is never index degradation.
pub(crate) fn protect_snapshot(state: &AppState, path: &str, content: &str) -> AppResult<i64> {
    let hash = content_hash_str(content);
    let snapshot = crate::version::create_snapshot_under_move_lock(
        state,
        path,
        content,
        crate::version::SnapshotParams::manual(),
    )?
    .entry;
    let id = snapshot
        .map(|entry| entry.id)
        .or_else(|| {
            crate::version::version_list(state, path)
                .ok()?
                .into_iter()
                .find(|entry| !entry.is_legacy_unscoped && entry.content_hash == hash)
                .map(|entry| entry.id)
        })
        .ok_or_else(|| AppError::msg("note_recovery_snapshot_missing"))?;
    if content_hash_str(&crate::version::version_preview(state, id)?) != hash {
        return Err(AppError::msg("note_recovery_snapshot_invalid"));
    }
    Ok(id)
}

/// Create using an explicit vault and an absence precondition, never overwrite.
pub(crate) fn create_note(
    state: &AppState,
    expected_vault: &Path,
    path: &str,
    content: &str,
    validate: impl FnOnce() -> AppResult<()>,
) -> AppResult<FileWriteResult> {
    with_vault_move_lock(|| {
        if state.vault_path()? != expected_vault {
            return Err(AppError::msg("note_vault_changed"));
        }
        if !crate::storage::paths::is_user_note_path(path)
            || !path.ends_with(".md")
            || content.len() > 20 * 1024 * 1024
        {
            return Err(AppError::msg("invalid_note_create"));
        }
        validate()?;
        NoteWriteService::create_under_move_lock(state, path, content)
    })
}

/// Recycle a specific read baseline; actual recovery identity comes from the recycle module.
pub(crate) fn trash_note(
    state: &AppState,
    expected_vault: &Path,
    path: &str,
    base_hash: &str,
    validate: impl FnOnce() -> AppResult<()>,
) -> AppResult<crate::recycle::TrashReceipt> {
    with_vault_move_lock(|| {
        if state.vault_path()? != expected_vault {
            return Err(AppError::msg("note_vault_changed"));
        }
        validate()?;
        let absolute = validate_user_note_relative_path(expected_vault, path)?;
        if content_hash_str(&std::fs::read_to_string(absolute)?) != base_hash {
            return Err(AppError::msg("note_content_conflict"));
        }
        crate::recycle::trash_locked(state, path)
    })
}

#[cfg(test)]
mod tests {
    use super::{edited_content, NoteEdit};
    use crate::cas::hash::content_hash_str;

    #[test]
    fn exact_edit_preserves_unrelated_markdown_bytes() {
        let before = "\u{feff}---\r\ntitle: '原题'\r\ntags: [one, two]\r\n---\r\n\r\n# 标题\r\n\r\n目标短句  \r\n\r\n```rust\r\nlet text = \"目标短句\";\r\n```\r\n\r\n[[目标短句|别名]] [链接](./目标短句.md)\r\n\r\n<!-- 保留 -->";
        let start = before.find("目标短句").unwrap();
        let edit = NoteEdit {
            path: "note.md".into(),
            base_content_hash: content_hash_str(before),
            range: start..start + "目标短句".len(),
            original: "目标短句".into(),
            replacement: "新的正文🙂".into(),
        };
        let after = edited_content(&edit, before).unwrap();
        assert_eq!(after, before.replacen("目标短句", "新的正文🙂", 1));
        assert_eq!(&after[..start], &before[..start]);
        assert_eq!(
            &after[start + edit.replacement.len()..],
            &before[edit.range.end..]
        );
    }

    #[test]
    fn exact_edit_rejects_invalid_utf8_ranges_and_original_without_panicking() {
        let before = "正文🙂正文";
        for (range, original) in [
            (1..3, "正文"),
            (0..usize::MAX, "正文"),
            (std::ops::Range { start: 8, end: 2 }, ""),
            (0..6, "别的"),
        ] {
            let edit = NoteEdit {
                path: "note.md".into(),
                base_content_hash: content_hash_str(before),
                range,
                original: original.into(),
                replacement: "替换".into(),
            };
            assert!(edited_content(&edit, before).is_err());
        }
    }

    #[test]
    fn exact_edit_rejects_stale_baseline_even_if_original_still_matches() {
        let edit = NoteEdit {
            path: "note.md".into(),
            base_content_hash: content_hash_str("正文\n"),
            range: 0..6,
            original: "正文".into(),
            replacement: "替换".into(),
        };
        assert!(edited_content(&edit, "正文\n外部编辑").is_err());
    }
}
