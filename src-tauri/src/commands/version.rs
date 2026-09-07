use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::app::AppState;
use crate::error::AppResult;
use crate::version::{self, VersionEntry};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionSaveResult {
    pub created: bool,
    pub version_id: Option<i64>,
    pub skip_reason: Option<String>,
}

#[derive(Clone, Copy)]
enum VersionSaveKind {
    Manual,
    Idle,
}

impl VersionSaveKind {
    fn run(
        self,
        state: &Arc<AppState>,
        expected_vault: &Path,
        path: &str,
        content: &str,
    ) -> AppResult<version::VersionSaveOutcome> {
        let params = match self {
            Self::Manual => version::SnapshotParams::manual(),
            Self::Idle => version::SnapshotParams::auto_idle(),
        };
        version::create_snapshot_outcome_in_vault(state, expected_vault, path, content, params)
    }
}

async fn run_version_save(
    state: Arc<AppState>,
    path: String,
    content: String,
    kind: VersionSaveKind,
    expected_vault: Option<String>,
) -> AppResult<VersionSaveResult> {
    let captured_vault = capture_expected_vault(&state, expected_vault.as_deref())?;
    let outcome =
        tokio::task::spawn_blocking(move || kind.run(&state, &captured_vault, &path, &content))
            .await
            .map_err(|e| crate::error::AppError::msg(format!("version save task failed: {e}")))??;
    Ok(VersionSaveResult {
        created: outcome.entry.is_some(),
        version_id: outcome.entry.map(|entry| entry.id),
        skip_reason: outcome
            .skip_reason
            .map(|reason| reason.as_str().to_string()),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct VersionRestoreResult {
    pub content: String,
}

#[tauri::command]
pub fn version_list_cmd(
    state: State<'_, Arc<AppState>>,
    path: String,
    expected_vault: Option<String>,
    include_legacy_unassigned: Option<bool>,
) -> AppResult<Vec<VersionEntry>> {
    crate::storage::atomic_write::with_vault_move_lock(|| {
        check_expected_vault(&state, expected_vault.as_deref())?;
        version::version_list_including_unassigned(
            &state,
            &path,
            include_legacy_unassigned.unwrap_or(false),
        )
    })
}

#[tauri::command]
pub fn version_preview_cmd(
    state: State<'_, Arc<AppState>>,
    version_id: i64,
    expected_vault: Option<String>,
) -> AppResult<String> {
    crate::storage::atomic_write::with_vault_move_lock(|| {
        check_expected_vault(&state, expected_vault.as_deref())?;
        version::version_preview(&state, version_id)
    })
}

fn check_expected_vault(state: &AppState, expected: Option<&str>) -> AppResult<()> {
    capture_expected_vault(state, expected).map(|_| ())
}

fn capture_expected_vault(state: &AppState, expected: Option<&str>) -> AppResult<PathBuf> {
    let vault = state.vault_path()?;
    if expected.is_some_and(|expected| vault.to_string_lossy() != expected) {
        return Err(crate::error::AppError::msg("version_vault_changed"));
    }
    Ok(vault)
}

#[tauri::command]
pub fn version_restore_cmd(
    state: State<'_, Arc<AppState>>,
    version_id: i64,
    current_content: String,
    target_path: Option<String>,
    expected_vault: Option<String>,
    allow_legacy_unscoped: Option<bool>,
) -> AppResult<VersionRestoreResult> {
    let content = version::version_restore_scoped(
        &state,
        version_id,
        &current_content,
        target_path.as_deref(),
        expected_vault.as_deref(),
        allow_legacy_unscoped.unwrap_or(false),
    )?;
    Ok(VersionRestoreResult { content })
}

#[tauri::command]
pub fn version_delete_cmd(
    state: State<'_, Arc<AppState>>,
    version_id: i64,
    expected_vault: Option<String>,
) -> AppResult<()> {
    crate::storage::atomic_write::with_vault_move_lock(|| {
        check_expected_vault(&state, expected_vault.as_deref())?;
        version::version_delete_under_move_lock(&state, version_id)
    })
}

#[tauri::command]
pub fn version_finalize_current_cmd(
    state: State<'_, Arc<AppState>>,
    path: String,
    content: String,
    label: Option<String>,
    expected_vault: Option<String>,
) -> AppResult<Option<VersionEntry>> {
    let vault = capture_expected_vault(&state, expected_vault.as_deref())?;
    Ok(version::create_snapshot_outcome_in_vault(
        &state,
        &vault,
        &path,
        &content,
        version::SnapshotParams::finalize(label),
    )?
    .entry)
}

/// Creates a manual snapshot and returns its durable result.
#[tauri::command]
pub async fn version_save_manual_cmd(
    state: State<'_, Arc<AppState>>,
    path: String,
    content: String,
    expected_vault: Option<String>,
) -> AppResult<VersionSaveResult> {
    run_version_save(
        state.inner().clone(),
        path,
        content,
        VersionSaveKind::Manual,
        expected_vault,
    )
    .await
}

/// Creates an idle snapshot and returns its durable result.
#[tauri::command]
pub async fn version_save_idle_cmd(
    state: State<'_, Arc<AppState>>,
    path: String,
    content: String,
    expected_vault: Option<String>,
) -> AppResult<VersionSaveResult> {
    run_version_save(
        state.inner().clone(),
        path,
        content,
        VersionSaveKind::Idle,
        expected_vault,
    )
    .await
}

/// Creates a pre-close snapshot (bypasses idle cooldown / hash dedup).
#[tauri::command]
pub fn version_save_pre_close_cmd(
    state: State<'_, Arc<AppState>>,
    path: String,
    content: String,
    expected_vault: Option<String>,
) -> AppResult<VersionSaveResult> {
    let vault = capture_expected_vault(&state, expected_vault.as_deref())?;
    let entry = version::create_snapshot_outcome_in_vault(
        &state,
        &vault,
        &path,
        &content,
        version::SnapshotParams::pre_close(),
    )?
    .entry;
    Ok(VersionSaveResult {
        created: entry.is_some(),
        version_id: entry.map(|e| e.id),
        skip_reason: None,
    })
}
