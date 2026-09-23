//! C24 bounded, on-demand unified diff for one pending frozen change plan.
//!
//! Input: the session-owned pending confirmation identity plus the persisted
//! frozen plan it names. Output: bounded unified diff hunks replayed from the
//! frozen candidates against the current original bodies.
//!
//! Privacy boundary: the preview is a transient IPC response for the owning
//! user's own review. Note prose computed here is never inserted into
//! persisted run events, tool call results, audit records, or logs; those keep
//! projecting only `summary` + `targets`. The preview does not write, approve,
//! or extend a change plan — generate ≠ write holds.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ai_runtime::edit_candidate::prepare_edit_candidate;
use crate::ai_runtime::frozen_change_plan::FrozenChangePlan;
use crate::ai_runtime::run_contract::{AssistantSessionRef, SafeRunErrorCode, SecurityDomain};
use crate::app::AppState;
use crate::error::{AppError, AppResult};

/// Request accepted by `assistant_run_confirmation_diff`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantRunConfirmationDiffRequest {
    /// Session that owns the Run.
    pub(crate) session: AssistantSessionRef,
    /// Stable Run identifier that owns the pending confirmation.
    pub(crate) run_id: String,
    /// Pending confirmation identifier observed in the confirmation event.
    pub(crate) confirmation_id: String,
    /// Plan identity observed in the confirmation event.
    pub(crate) plan_hash: String,
}

/// On-demand, bounded unified diff for one pending frozen change plan.
///
/// Privacy boundary: this is a transient review response for the owning user.
/// Note prose here is never inserted into persisted run events, tool call
/// results, audit records, or logs; those keep projecting only `summary` and
/// `targets`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmationDiffPreview {
    /// Per-target diffs in frozen first-use order.
    pub(crate) files: Vec<ConfirmationFileDiff>,
    /// True when hunk or line bounds dropped the remaining difference.
    pub(crate) truncated: bool,
}

/// Unified diff projection for exactly one frozen change target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmationFileDiff {
    /// Normalized vault-relative path of the affected note.
    pub(crate) path: String,
    /// False when the frozen candidate cannot be replayed for display.
    pub(crate) previewable: bool,
    /// Bounded unified diff hunks; empty when not previewable.
    pub(crate) hunks: Vec<ConfirmationDiffHunk>,
}

/// One bounded region of changed lines with surrounding context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmationDiffHunk {
    /// One-based start line in the frozen original, 0 for pure insertions.
    pub(crate) old_start: u32,
    /// One-based start line in the candidate body, 0 for pure deletions.
    pub(crate) new_start: u32,
    /// Ordered unified diff lines without trailing newlines.
    pub(crate) lines: Vec<ConfirmationDiffLine>,
}

/// Unified diff line classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmationDiffLineKind {
    /// Unchanged context line.
    Context,
    /// Line present only in the candidate body.
    Add,
    /// Line present only in the frozen original.
    Del,
}

/// One visible line of a bounded unified diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmationDiffLine {
    /// Unified diff classification.
    pub(crate) kind: ConfirmationDiffLineKind,
    /// Line text without the trailing newline.
    pub(crate) text: String,
}

/// One persisted frozen plan row that is still awaiting user confirmation.
/// Storage-shaped: `plan_json` only rebuilds the bounded review projection and
/// never crosses the IPC boundary verbatim.
struct PendingFrozenConfirmation {
    plan_hash: String,
    plan_json: String,
    expires_at_unix_ms: i64,
}

/// Load the exact pending plan for the session that owns the Run. Expiry is
/// reported to the caller instead of being filtered in SQL, so a stale approval
/// window is distinguishable from a missing confirmation.
fn pending_frozen_confirmation_for_session(
    state: &AppState,
    session_key: &str,
    run_id: &str,
    confirmation_id: &str,
) -> AppResult<PendingFrozenConfirmation> {
    state.db.with_read_conn(|conn| {
        conn.query_row(
            "SELECT c.plan_hash, c.plan_json, c.expires_at
             FROM agent_run_confirmations c
             JOIN agent_runs r ON r.run_id = c.run_id
             JOIN sessions s ON s.id = r.session_id
             WHERE c.run_id = ?1 AND c.confirmation_id = ?2
               AND c.status = 'pending' AND s.session_key = ?3",
            rusqlite::params![run_id, confirmation_id, session_key],
            |row| {
                Ok(PendingFrozenConfirmation {
                    plan_hash: row.get(0)?,
                    plan_json: row.get(1)?,
                    expires_at_unix_ms: row.get(2)?,
                })
            },
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::run(SafeRunErrorCode::ConfirmationDiffUnavailable)
            }
            other => AppError::from(other),
        })
    })
}

/// Unchanged context lines kept around each change cluster.
const DIFF_CONTEXT_LINES: usize = 2;
/// Upper bound on emitted hunks across all files.
const MAX_DIFF_HUNKS: usize = 50;
/// Upper bound on emitted line-text characters across all files.
const MAX_DIFF_CHARS: usize = 20_000;

/// Line-level unified diff between the frozen original and the candidate body.
///
/// Line texts never carry trailing newlines. Hunks merge change clusters whose
/// context windows touch, so a hunk is the smallest independently readable
/// region of the difference.
pub(crate) fn diff_bodies(original: &str, candidate: &str) -> Vec<ConfirmationDiffHunk> {
    let diff = similar::TextDiff::from_lines(original, candidate);
    let mut flat: Vec<(ConfirmationDiffLine, Option<u32>, Option<u32>)> = Vec::new();
    let mut old_line = 1u32;
    let mut new_line = 1u32;
    for change in diff.iter_all_changes() {
        let kind = match change.tag() {
            similar::ChangeTag::Equal => ConfirmationDiffLineKind::Context,
            similar::ChangeTag::Delete => ConfirmationDiffLineKind::Del,
            similar::ChangeTag::Insert => ConfirmationDiffLineKind::Add,
        };
        let (old_no, new_no) = match kind {
            ConfirmationDiffLineKind::Context => {
                let pair = (Some(old_line), Some(new_line));
                old_line += 1;
                new_line += 1;
                pair
            }
            ConfirmationDiffLineKind::Del => {
                let pair = (Some(old_line), None);
                old_line += 1;
                pair
            }
            ConfirmationDiffLineKind::Add => {
                let pair = (None, Some(new_line));
                new_line += 1;
                pair
            }
        };
        let text = change.value().trim_end_matches(['\n', '\r']).to_string();
        flat.push((ConfirmationDiffLine { kind, text }, old_no, new_no));
    }
    cluster_into_hunks(&flat)
}

fn cluster_into_hunks(
    flat: &[(ConfirmationDiffLine, Option<u32>, Option<u32>)],
) -> Vec<ConfirmationDiffHunk> {
    let mut groups: Vec<(usize, usize)> = Vec::new();
    for (index, (line, _, _)) in flat.iter().enumerate() {
        if line.kind == ConfirmationDiffLineKind::Context {
            continue;
        }
        match groups.last_mut() {
            Some(group) if index.saturating_sub(group.1) <= 2 * DIFF_CONTEXT_LINES + 1 => {
                group.1 = index;
            }
            _ => groups.push((index, index)),
        }
    }
    groups
        .into_iter()
        .map(|(first, last)| {
            let start = first.saturating_sub(DIFF_CONTEXT_LINES);
            let end = (last + 1 + DIFF_CONTEXT_LINES).min(flat.len());
            let slice = &flat[start..end];
            let old_start = slice.iter().find_map(|(_, old_no, _)| *old_no).unwrap_or(0);
            let new_start = slice.iter().find_map(|(_, _, new_no)| *new_no).unwrap_or(0);
            ConfirmationDiffHunk {
                old_start,
                new_start,
                lines: slice
                    .iter()
                    .map(|(line, _, _)| ConfirmationDiffLine {
                        kind: line.kind,
                        text: line.text.clone(),
                    })
                    .collect(),
            }
        })
        .collect()
}

/// Replay every frozen edit candidate and diff each target's original body
/// against its final chained candidate. Targets that cannot be replayed (disk
/// drift, unreadable note, non-edit operations) are returned with
/// `previewable: false` instead of failing the whole preview.
pub(crate) fn build_confirmation_diff(
    plan: &FrozenChangePlan,
    vault: &Path,
) -> ConfirmationDiffPreview {
    let mut originals: BTreeMap<String, String> = BTreeMap::new();
    let mut virtual_documents: BTreeMap<String, String> = BTreeMap::new();
    let mut failed: BTreeSet<String> = BTreeSet::new();
    for operation in plan.operations() {
        let is_edit = matches!(
            operation.operation(),
            "replace_selection" | "insert_text_at_cursor"
        );
        if !is_edit {
            failed.extend(operation.relative_paths().iter().cloned());
            continue;
        }
        let Some(path) = operation.relative_paths().first() else {
            continue;
        };
        if failed.contains(path) {
            continue;
        }
        let original = match virtual_documents.get(path) {
            Some(body) => body.clone(),
            None => match read_original(vault, path) {
                Some(body) => {
                    originals
                        .entry(path.clone())
                        .or_insert_with(|| body.clone());
                    body
                }
                None => {
                    failed.insert(path.clone());
                    continue;
                }
            },
        };
        let preview_args = replay_args_for_candidate(operation.change(), path);
        match prepare_edit_candidate(operation.operation(), &preview_args, &original) {
            Ok(candidate) => {
                virtual_documents.insert(path.clone(), candidate.candidate_body);
            }
            Err(_) => {
                failed.insert(path.clone());
            }
        }
    }

    let mut files = Vec::new();
    let mut truncated = false;
    let mut hunk_budget = MAX_DIFF_HUNKS;
    let mut char_budget = MAX_DIFF_CHARS;
    for path in plan.relative_paths() {
        if failed.contains(path) {
            files.push(ConfirmationFileDiff {
                path: path.clone(),
                previewable: false,
                hunks: Vec::new(),
            });
            continue;
        }
        let (Some(original), Some(final_body)) = (originals.get(path), virtual_documents.get(path))
        else {
            files.push(ConfirmationFileDiff {
                path: path.clone(),
                previewable: false,
                hunks: Vec::new(),
            });
            continue;
        };
        let mut hunks = diff_bodies(original, final_body);
        if hunks.len() > hunk_budget {
            hunks.truncate(hunk_budget);
        }
        hunk_budget = hunk_budget.saturating_sub(hunks.len());
        let mut kept_chars = 0usize;
        for hunk in &mut hunks {
            let mut kept_lines = 0usize;
            for line in &hunk.lines {
                if kept_chars + line.text.chars().count() > char_budget {
                    break;
                }
                kept_chars += line.text.chars().count();
                kept_lines += 1;
            }
            if kept_lines < hunk.lines.len() {
                hunk.lines.truncate(kept_lines);
                truncated = true;
            }
        }
        hunks.retain(|hunk| !hunk.lines.is_empty());
        if hunk_budget == 0 || kept_chars >= char_budget {
            truncated |= has_remaining_changes(&hunks, original, final_body);
        }
        char_budget = char_budget.saturating_sub(kept_chars);
        files.push(ConfirmationFileDiff {
            path: path.clone(),
            previewable: true,
            hunks,
        });
    }
    ConfirmationDiffPreview { files, truncated }
}

/// Whether the emitted hunks stopped short of the full difference.
fn has_remaining_changes(
    emitted: &[ConfirmationDiffHunk],
    original: &str,
    final_body: &str,
) -> bool {
    let total = diff_bodies(original, final_body).len();
    emitted.len() < total
}

/// Fill the frozen change arguments exactly as the freeze path did, so the
/// replayed candidate is byte-identical to the frozen one.
fn replay_args_for_candidate(change: &serde_json::Value, path: &str) -> serde_json::Value {
    let mut preview = change.clone();
    let has_path = ["target_path", "path"].iter().any(|key| {
        preview
            .get(*key)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
    });
    if !has_path {
        preview["target_path"] = serde_json::json!(path);
    }
    preview
}

fn read_original(vault: &Path, path: &str) -> Option<String> {
    let resolved = crate::storage::paths::validate_user_note_relative_path(vault, path).ok()?;
    std::fs::read_to_string(resolved).ok()
}

/// Session-owned entry: project a bounded unified diff for exactly one pending
/// frozen confirmation. Read-only and transient — no events, no audit rows, no
/// persistence side effects of any kind.
pub(crate) fn preview_pending_confirmation_diff(
    state: &AppState,
    request: AssistantRunConfirmationDiffRequest,
) -> AppResult<ConfirmationDiffPreview> {
    if request.session.domain != SecurityDomain::Normal {
        return Err(AppError::run(SafeRunErrorCode::ControlNotAvailable));
    }
    let pending = pending_frozen_confirmation_for_session(
        state,
        &request.session.session_key,
        &request.run_id,
        &request.confirmation_id,
    )?;
    if pending.plan_hash != request.plan_hash {
        return Err(AppError::run(
            SafeRunErrorCode::ConfirmationPlanHashMismatch,
        ));
    }
    if pending.expires_at_unix_ms < chrono::Utc::now().timestamp_millis() {
        return Err(AppError::run(SafeRunErrorCode::ConfirmationExpired));
    }
    let plan = FrozenChangePlan::from_persisted_plan_json(&pending.plan_json)?;
    plan.validate_consumed_identity(&request.confirmation_id, &request.plan_hash)?;
    let vault = state
        .vault_path()
        .map_err(|_| AppError::run(SafeRunErrorCode::ConfirmationDiffUnavailable))?;
    Ok(build_confirmation_diff(&plan, &vault))
}
