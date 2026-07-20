use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::indexer::frontmatter::parse_note;
use crate::indexer::scan::content_hash;
use crate::storage::note_title::{is_placeholder_title, title_from_path};
use crate::storage::note_write::{FileWriteResult, NoteWriteService};
use crate::storage::paths::{is_user_note_path, read_file_lossy, resolve_vault_path};

const RECOVERY_PREVIEW_CHARS: usize = 800;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentTitleAuditItem {
    pub path: String,
    pub current_title: String,
    pub candidate_title: Option<String>,
    pub candidate_source: Option<String>,
    pub content_hash: Option<String>,
    pub reason: String,
}

/// A missing Markdown file whose content can be recreated from a retained version.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingDocumentRecoveryItem {
    pub path: String,
    pub current_title: String,
    pub candidate_title: Option<String>,
    pub version_id: i64,
    pub content_hash: String,
    pub created_at: String,
    pub preview: String,
}

/// An unattached, content-addressed Markdown blob that is not referenced by a live version.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrphanedDocumentRecoveryItem {
    pub object_hash: String,
    pub candidate_title: Option<String>,
    pub suggested_path: String,
    pub preview: String,
}

/// An indexed missing document for which Iris has no safe local content source.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnavailableDocumentRecoveryItem {
    pub path: String,
    pub current_title: String,
    pub reason: String,
}

/// Complete read-only recovery audit. No Markdown is changed while producing it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentRecoveryAudit {
    pub title_issues: Vec<DocumentTitleAuditItem>,
    pub missing_documents: Vec<MissingDocumentRecoveryItem>,
    pub orphaned_documents: Vec<OrphanedDocumentRecoveryItem>,
    pub unavailable_documents: Vec<UnavailableDocumentRecoveryItem>,
}

fn safe_vault_join(vault: &Path, relative: &str) -> AppResult<PathBuf> {
    if !is_user_note_path(relative) {
        return Err(AppError::msg("document recovery path is not a user note"));
    }
    let mut joined = vault.to_path_buf();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => joined.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::msg("document recovery path traversal rejected"));
            }
        }
    }
    Ok(joined)
}

/// Returns `None` when the indexed note is missing, including when its parent
/// directory was removed. Existing files still go through canonical resolution.
fn existing_note_path(vault: &Path, path: &str) -> AppResult<Option<PathBuf>> {
    let candidate = safe_vault_join(vault, path)?;
    if !candidate.is_file() {
        return Ok(None);
    }
    Ok(Some(resolve_vault_path(vault, path)?))
}

fn preview(content: &str) -> String {
    let mut chars = content.chars();
    let value: String = chars.by_ref().take(RECOVERY_PREVIEW_CHARS).collect();
    if chars.next().is_some() {
        format!("{value}\n…")
    } else {
        value
    }
}

fn title_from_content(content: &str) -> Option<String> {
    parse_note(content)
        .ok()?
        .title
        .filter(|title| !is_placeholder_title(title))
}

fn candidate_title(content: &str, stored_title: &str, path: &str) -> Option<String> {
    title_from_content(content)
        .or_else(|| (!is_placeholder_title(stored_title)).then(|| stored_title.trim().to_string()))
        .or_else(|| {
            let filename = title_from_path(path);
            (!is_placeholder_title(&filename)).then_some(filename)
        })
}

fn history_title(state: &AppState, path: &str) -> Option<String> {
    let versions = crate::version::version_list(state, path).ok()?;
    for version in versions {
        let content = crate::version::version_preview(state, version.id).ok()?;
        if let Some(title) = title_from_content(&content) {
            return Some(title);
        }
    }
    None
}

fn audit_titles(state: &AppState) -> AppResult<Vec<DocumentTitleAuditItem>> {
    let vault = state.vault_path()?;
    let indexed: Vec<(String, String)> = state.db.with_read_conn(|conn| {
        let mut statement = conn.prepare("SELECT path, title FROM files ORDER BY path")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.flatten().collect())
    })?;

    let mut findings = Vec::new();
    for (path, stored_title) in indexed {
        if !is_user_note_path(&path) {
            continue;
        }
        let Some(absolute) = existing_note_path(&vault, &path)? else {
            findings.push(DocumentTitleAuditItem {
                path,
                current_title: stored_title,
                candidate_title: None,
                candidate_source: None,
                content_hash: None,
                reason: "missing_markdown".to_string(),
            });
            continue;
        };
        let content = read_file_lossy(&absolute)?;
        let parsed = match parse_note(&content) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };
        if parsed.body.trim().is_empty() {
            continue;
        }
        let current_title = parsed.title.unwrap_or_default();
        let title_is_suspicious = is_placeholder_title(&current_title);
        let index_disagrees = !current_title.is_empty()
            && !stored_title.trim().is_empty()
            && current_title.trim() != stored_title.trim();
        if !title_is_suspicious && !index_disagrees {
            continue;
        }

        let (candidate_title, candidate_source) = if let Some(title) = history_title(state, &path) {
            (Some(title), Some("version".to_string()))
        } else if !is_placeholder_title(&stored_title) {
            (Some(stored_title.clone()), Some("index".to_string()))
        } else {
            let filename = title_from_path(&path);
            if is_placeholder_title(&filename) {
                (None, None)
            } else {
                (Some(filename), Some("filename".to_string()))
            }
        };
        findings.push(DocumentTitleAuditItem {
            path,
            current_title,
            candidate_title,
            candidate_source,
            content_hash: Some(content_hash(&content)),
            reason: if title_is_suspicious {
                "missing_or_placeholder_title".to_string()
            } else {
                "index_title_mismatch".to_string()
            },
        });
    }
    Ok(findings)
}

fn audit_missing_documents(
    state: &AppState,
) -> AppResult<(
    Vec<MissingDocumentRecoveryItem>,
    Vec<UnavailableDocumentRecoveryItem>,
)> {
    let vault = state.vault_path()?;
    let indexed: Vec<(String, String)> = state.db.with_read_conn(|conn| {
        let mut statement = conn.prepare("SELECT path, title FROM files ORDER BY path")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.flatten().collect())
    })?;

    let mut recoverable = Vec::new();
    let mut unavailable = Vec::new();
    for (path, stored_title) in indexed {
        if !is_user_note_path(&path) {
            continue;
        }
        if existing_note_path(&vault, &path)?.is_some() {
            continue;
        }
        let mut recovered = None;
        for version in crate::version::version_list(state, &path)? {
            let Ok(content) = crate::version::version_preview(state, version.id) else {
                continue;
            };
            if crate::cas::hash::content_hash_str(&content) != version.content_hash {
                continue;
            }
            recovered = Some(MissingDocumentRecoveryItem {
                path: path.clone(),
                current_title: stored_title.clone(),
                candidate_title: candidate_title(&content, &stored_title, &path),
                version_id: version.id,
                content_hash: version.content_hash,
                created_at: version.created_at,
                preview: preview(&content),
            });
            break;
        }
        if let Some(item) = recovered {
            recoverable.push(item);
        } else {
            unavailable.push(UnavailableDocumentRecoveryItem {
                path,
                current_title: stored_title,
                reason: "no_readable_version_snapshot".to_string(),
            });
        }
    }
    Ok((recoverable, unavailable))
}

fn valid_object_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// A version diff is a CAS implementation detail, never a standalone document.
/// It may be left behind after a version is discarded, but restoring it as Markdown
/// would corrupt the user's vault rather than recover a note.
fn is_unified_diff(content: &str) -> bool {
    content.starts_with("--- ")
        && content.lines().any(|line| line.starts_with("+++ "))
        && content.lines().any(|line| line.starts_with("@@ "))
}

fn active_version_object_hashes(state: &AppState) -> AppResult<HashSet<String>> {
    let storage_paths: Vec<String> = state.db.with_read_conn(|conn| {
        let mut statement = conn.prepare("SELECT storage_path FROM versions")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        Ok(rows.flatten().collect())
    })?;
    let mut hashes = HashSet::new();
    for storage_path in storage_paths {
        if let Some(hash) = storage_path.strip_prefix("cas:") {
            if valid_object_hash(hash) {
                hashes.insert(hash.to_string());
            }
        } else if let Some(diff) = storage_path.strip_prefix("dif:") {
            if let Some((parent, patch)) = diff.split_once(':') {
                if valid_object_hash(parent) {
                    hashes.insert(parent.to_string());
                }
                if valid_object_hash(patch) {
                    hashes.insert(patch.to_string());
                }
            }
        }
    }
    Ok(hashes)
}

fn orphan_document_content(state: &AppState, object_hash: &str) -> AppResult<String> {
    if !valid_object_hash(object_hash) {
        return Err(AppError::msg("invalid CAS object hash"));
    }
    if active_version_object_hashes(state)?.contains(object_hash) {
        return Err(AppError::msg(
            "CAS object is referenced by a live version; run the audit again",
        ));
    }
    let store = state.cas_store()?;
    if store.read_tree(object_hash).is_ok() || store.read_commit(object_hash).is_ok() {
        return Err(AppError::msg(
            "CAS object is metadata, not a Markdown document",
        ));
    }
    let content = store.read_blob_content(object_hash)?;
    if crate::cas::hash::content_hash_str(&content) != object_hash
        || content.trim().is_empty()
        || is_unified_diff(&content)
    {
        return Err(AppError::msg(
            "CAS object is not a recoverable Markdown snapshot",
        ));
    }
    Ok(content)
}

fn audit_orphaned_documents(state: &AppState) -> AppResult<Vec<OrphanedDocumentRecoveryItem>> {
    let active = active_version_object_hashes(state)?;
    let store = state.cas_store()?;
    let mut findings = Vec::new();
    for object_hash in store.list_object_hashes()? {
        if active.contains(&object_hash) {
            continue;
        }
        let Ok(content) = orphan_document_content(state, &object_hash) else {
            continue;
        };
        findings.push(OrphanedDocumentRecoveryItem {
            suggested_path: format!("Recovered/{}.md", &object_hash[..12]),
            candidate_title: title_from_content(&content),
            preview: preview(&content),
            object_hash,
        });
    }
    Ok(findings)
}

fn audit_document_recovery(state: &AppState) -> AppResult<DocumentRecoveryAudit> {
    let title_issues = audit_titles(state)?
        .into_iter()
        .filter(|item| item.reason != "missing_markdown")
        .collect();
    let (missing_documents, unavailable_documents) = audit_missing_documents(state)?;
    let orphaned_documents = audit_orphaned_documents(state)?;
    Ok(DocumentRecoveryAudit {
        title_issues,
        missing_documents,
        orphaned_documents,
        unavailable_documents,
    })
}

fn restore_missing_document(
    state: &AppState,
    path: &str,
    version_id: i64,
    expected_content_hash: &str,
) -> AppResult<FileWriteResult> {
    if !is_user_note_path(path) || !path.to_ascii_lowercase().ends_with(".md") {
        return Err(AppError::msg("invalid missing-document recovery path"));
    }
    let stored_hash: String = state.db.with_read_conn(|conn| {
        conn.query_row(
            "SELECT v.content_hash
             FROM versions v JOIN files f ON f.id = v.file_id
             WHERE v.id = ?1 AND f.path = ?2",
            rusqlite::params![version_id, path],
            |row| row.get(0),
        )
        .map_err(Into::into)
    })?;
    if stored_hash != expected_content_hash {
        return Err(AppError::msg(
            "recovery snapshot changed; run the audit again",
        ));
    }
    let content = crate::version::version_preview(state, version_id)?;
    if crate::cas::hash::content_hash_str(&content) != expected_content_hash {
        return Err(AppError::msg("recovery snapshot integrity check failed"));
    }
    // `create` is atomic and refuses to overwrite a newly recreated file.
    NoteWriteService::create(state, path, &content)
}

fn restore_orphaned_document(
    state: &AppState,
    object_hash: &str,
    target_path: &str,
) -> AppResult<FileWriteResult> {
    if !is_user_note_path(target_path) || !target_path.to_ascii_lowercase().ends_with(".md") {
        return Err(AppError::msg("invalid orphan-document recovery path"));
    }
    let content = orphan_document_content(state, object_hash)?;
    // `create` is atomic and preserves any file created after the audit.
    NoteWriteService::create(state, target_path, &content)
}

/// Read-only audit for title corruption; no Markdown is changed by this command.
#[tauri::command]
pub async fn document_title_audit_cmd(
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<DocumentTitleAuditItem>> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || audit_titles(&state))
        .await
        .map_err(|error| AppError::msg(format!("title audit task failed: {error}")))?
}

/// Read-only audit for missing indexed documents and unattached Markdown CAS blobs.
#[tauri::command]
pub async fn document_recovery_audit_cmd(
    state: State<'_, Arc<AppState>>,
) -> AppResult<DocumentRecoveryAudit> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || audit_document_recovery(&state))
        .await
        .map_err(|error| AppError::msg(format!("document recovery audit task failed: {error}")))?
}

/// Restores a missing Markdown document from one audit-identified version snapshot.
#[tauri::command]
pub async fn document_recovery_restore_missing_cmd(
    state: State<'_, Arc<AppState>>,
    path: String,
    version_id: i64,
    expected_content_hash: String,
    confirmed: bool,
) -> AppResult<FileWriteResult> {
    if !confirmed {
        return Err(AppError::msg(
            "missing-document recovery requires explicit confirmation",
        ));
    }
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        restore_missing_document(&state, &path, version_id, &expected_content_hash)
    })
    .await
    .map_err(|error| AppError::msg(format!("missing-document recovery task failed: {error}")))?
}

/// Restores an audit-identified, unattached Markdown CAS blob to a new user-selected path.
#[tauri::command]
pub async fn document_recovery_restore_orphan_cmd(
    state: State<'_, Arc<AppState>>,
    object_hash: String,
    target_path: String,
    confirmed: bool,
) -> AppResult<FileWriteResult> {
    if !confirmed {
        return Err(AppError::msg(
            "orphan-document recovery requires explicit confirmation",
        ));
    }
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        restore_orphaned_document(&state, &object_hash, &target_path)
    })
    .await
    .map_err(|error| AppError::msg(format!("orphan-document recovery task failed: {error}")))?
}

#[cfg(test)]
mod tests {
    use super::{audit_document_recovery, restore_missing_document, restore_orphaned_document};
    use crate::app::AppState;
    use crate::indexer::scan::scan_vault;
    use crate::version::version_save_manual;
    use std::fs;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup() -> (tempfile::TempDir, Arc<AppState>) {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let state = AppState::new(dir.path().join("data")).unwrap();
        state.set_vault(vault).unwrap();
        (dir, state)
    }

    #[test]
    fn restores_a_missing_indexed_document_from_its_version_without_overwrite() {
        let (_dir, state) = setup();
        let vault = state.vault_path().unwrap();
        let path = "notes/missing.md";
        let document = vault.join(path);
        fs::create_dir_all(document.parent().unwrap()).unwrap();
        fs::write(&document, "---\ntitle: \"Original\"\n---\n\nOriginal body").unwrap();
        state.db.with_conn(|conn| scan_vault(conn, &vault)).unwrap();
        let snapshot = "---\ntitle: \"Recovered\"\n---\n\nRecovered body";
        let version = version_save_manual(&state, path, snapshot)
            .unwrap()
            .expect("version snapshot");
        fs::remove_dir_all(document.parent().unwrap()).unwrap();

        let audit = audit_document_recovery(&state).unwrap();
        assert_eq!(audit.missing_documents.len(), 1);
        let item = &audit.missing_documents[0];
        assert_eq!(item.version_id, version.id);
        assert_eq!(item.content_hash, version.content_hash);

        restore_missing_document(&state, path, item.version_id, &item.content_hash).unwrap();
        assert_eq!(fs::read_to_string(&document).unwrap(), snapshot);
        assert_eq!(crate::version::version_list(&state, path).unwrap().len(), 1);
        assert!(
            restore_missing_document(&state, path, item.version_id, &item.content_hash).is_err()
        );
    }

    #[test]
    fn restores_an_unattached_markdown_cas_object_only_to_a_new_path() {
        let (_dir, state) = setup();
        let content = "---\ntitle: \"Orphan\"\n---\n\nRecovered from CAS";
        let object_hash = state.cas_store().unwrap().write_content(content).unwrap();

        let audit = audit_document_recovery(&state).unwrap();
        assert_eq!(audit.orphaned_documents.len(), 1);
        assert_eq!(audit.orphaned_documents[0].object_hash, object_hash);

        restore_orphaned_document(&state, &object_hash, "Recovered/orphan.md").unwrap();
        let vault = state.vault_path().unwrap();
        assert_eq!(
            fs::read_to_string(vault.join("Recovered/orphan.md")).unwrap(),
            content
        );
        assert!(restore_orphaned_document(&state, &object_hash, "Recovered/orphan.md").is_err());
    }

    #[test]
    fn does_not_offer_an_unattached_version_diff_as_a_document() {
        let (_dir, state) = setup();
        state
            .cas_store()
            .unwrap()
            .write_content(
                "--- a/notes/original.md\n+++ b/notes/original.md\n@@ -1 +1 @@\n-old\n+new\n",
            )
            .unwrap();

        let audit = audit_document_recovery(&state).unwrap();
        assert!(audit.orphaned_documents.is_empty());
    }
}
