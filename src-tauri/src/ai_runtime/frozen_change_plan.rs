//! Immutable, hash-bound confirmation payloads for change effects.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::ai_runtime::run_contract::SafeRunErrorCode;
use crate::ai_runtime::ToolCallResult;
use crate::error::{AppError, AppResult};

const MAX_CHANGE_OPERATIONS: usize = 6;
const MAX_CHANGE_TARGETS: usize = 6;

/// Legacy single-operation input retained solely so stored v1 plans and their
/// callers remain readable while confirmations migrate to change sets.
#[derive(Debug, Clone)]
pub(crate) struct FrozenChangePlanInput {
    pub(crate) confirmation_id: String,
    pub(crate) run_id: String,
    pub(crate) session_id: i64,
    pub(crate) request_id: String,
    pub(crate) tool_call_id: String,
    pub(crate) vault_id: String,
    pub(crate) relative_paths: Vec<String>,
    pub(crate) operation: String,
    pub(crate) base_content_hashes: Vec<(String, String)>,
    pub(crate) expected_post_content_hashes: Vec<(String, String)>,
    pub(crate) change: Value,
    pub(crate) affected_file_count: usize,
    pub(crate) rollback_summary: String,
    pub(crate) expires_at_unix_ms: i64,
}

/// One operation in an ordered, user-confirmed change set.
#[derive(Debug, Clone)]
pub(crate) struct FrozenChangeOperationInput {
    pub(crate) tool_call_id: String,
    pub(crate) operation: String,
    pub(crate) relative_paths: Vec<String>,
    pub(crate) base_content_hashes: Vec<(String, String)>,
    pub(crate) expected_post_content_hashes: Vec<(String, String)>,
    pub(crate) change: Value,
    pub(crate) rollback_summary: String,
}

/// Shared identity and ordered operation inputs for a v2 change set.
#[derive(Debug, Clone)]
pub(crate) struct FrozenChangeSetInput {
    pub(crate) confirmation_id: String,
    pub(crate) run_id: String,
    pub(crate) session_id: i64,
    pub(crate) request_id: String,
    pub(crate) vault_id: String,
    pub(crate) operations: Vec<FrozenChangeOperationInput>,
    pub(crate) expires_at_unix_ms: i64,
}

/// Immutable operation values exposed to dispatch and recovery.
#[derive(Debug, Clone)]
pub(crate) struct FrozenChangeOperation {
    input: FrozenChangeOperationInput,
}

impl FrozenChangeOperation {
    pub(crate) fn tool_call_id(&self) -> &str {
        &self.input.tool_call_id
    }
    pub(crate) fn operation(&self) -> &str {
        &self.input.operation
    }
    pub(crate) fn relative_paths(&self) -> &[String] {
        &self.input.relative_paths
    }
    pub(crate) fn base_content_hashes(&self) -> &[(String, String)] {
        &self.input.base_content_hashes
    }
    pub(crate) fn expected_post_content_hashes(&self) -> &[(String, String)] {
        &self.input.expected_post_content_hashes
    }
    pub(crate) fn change(&self) -> &Value {
        &self.input.change
    }
}

#[derive(Debug, Clone)]
enum FrozenChangePayload {
    Legacy(FrozenChangePlanInput),
    Set(FrozenChangeSetInput),
}

/// Frozen plan plus its canonical SHA-256 identity.
#[derive(Debug, Clone)]
pub(crate) struct FrozenChangePlan {
    payload: FrozenChangePayload,
    operations: Vec<FrozenChangeOperation>,
    relative_paths: Vec<String>,
    plan_hash: String,
}

impl FrozenChangePlan {
    /// Freeze a legacy single operation without changing its persisted hash.
    pub(crate) fn freeze(input: FrozenChangePlanInput) -> AppResult<Self> {
        validate_legacy_input(&input)?;
        let operation = FrozenChangeOperation {
            input: FrozenChangeOperationInput {
                tool_call_id: input.tool_call_id.clone(),
                operation: input.operation.clone(),
                relative_paths: input.relative_paths.clone(),
                base_content_hashes: input.base_content_hashes.clone(),
                expected_post_content_hashes: input.expected_post_content_hashes.clone(),
                change: input.change.clone(),
                rollback_summary: input.rollback_summary.clone(),
            },
        };
        Self::from_payload(FrozenChangePayload::Legacy(input), vec![operation])
    }

    /// Validate and freeze a bounded, ordered v2 change set.
    pub(crate) fn freeze_set(input: FrozenChangeSetInput) -> AppResult<Self> {
        if input.confirmation_id.trim().is_empty()
            || input.run_id.trim().is_empty()
            || input.request_id.trim().is_empty()
            || input.vault_id.trim().is_empty()
            || input.operations.is_empty()
            || input.operations.len() > MAX_CHANGE_OPERATIONS
        {
            return Err(AppError::run(SafeRunErrorCode::InvalidChangePlan));
        }
        let mut tool_call_ids = BTreeSet::new();
        let mut previous_expected = BTreeMap::<String, String>::new();
        for operation in &input.operations {
            validate_operation(operation)?;
            if !tool_call_ids.insert(operation.tool_call_id.as_str()) {
                return Err(AppError::run(SafeRunErrorCode::InvalidChangePlan));
            }
            for (path, base_hash) in &operation.base_content_hashes {
                if previous_expected
                    .get(path)
                    .is_some_and(|expected| expected != base_hash)
                {
                    return Err(AppError::run(SafeRunErrorCode::InvalidChangePlan));
                }
            }
            for (path, expected_hash) in &operation.expected_post_content_hashes {
                previous_expected.insert(path.clone(), expected_hash.clone());
            }
        }
        let operations = input
            .operations
            .iter()
            .cloned()
            .map(|input| FrozenChangeOperation { input })
            .collect::<Vec<_>>();
        let relative_paths = unique_relative_paths(&operations);
        if relative_paths.is_empty() || relative_paths.len() > MAX_CHANGE_TARGETS {
            return Err(AppError::run(SafeRunErrorCode::InvalidChangePlan));
        }
        Self::from_payload(FrozenChangePayload::Set(input), operations)
    }

    fn from_payload(
        payload: FrozenChangePayload,
        operations: Vec<FrozenChangeOperation>,
    ) -> AppResult<Self> {
        let relative_paths = unique_relative_paths(&operations);
        let canonical = canonical_json(&payload_value(&payload));
        let hash = Sha256::digest(canonical.as_bytes());
        Ok(Self {
            payload,
            operations,
            relative_paths,
            plan_hash: format!("sha256:{}", hex::encode(hash)),
        })
    }

    /// Stable hash shown to and returned by the user confirmation UI.
    pub(crate) fn plan_hash(&self) -> &str {
        &self.plan_hash
    }
    pub(crate) fn confirmation_id(&self) -> &str {
        match &self.payload {
            FrozenChangePayload::Legacy(i) => &i.confirmation_id,
            FrozenChangePayload::Set(i) => &i.confirmation_id,
        }
    }
    pub(crate) fn run_id(&self) -> &str {
        match &self.payload {
            FrozenChangePayload::Legacy(i) => &i.run_id,
            FrozenChangePayload::Set(i) => &i.run_id,
        }
    }
    pub(crate) fn vault_id(&self) -> &str {
        match &self.payload {
            FrozenChangePayload::Legacy(i) => &i.vault_id,
            FrozenChangePayload::Set(i) => &i.vault_id,
        }
    }
    pub(crate) const fn session_id(&self) -> i64 {
        match &self.payload {
            FrozenChangePayload::Legacy(i) => i.session_id,
            FrozenChangePayload::Set(i) => i.session_id,
        }
    }
    pub(crate) fn expires_at_unix_ms(&self) -> i64 {
        match &self.payload {
            FrozenChangePayload::Legacy(i) => i.expires_at_unix_ms,
            FrozenChangePayload::Set(i) => i.expires_at_unix_ms,
        }
    }
    pub(crate) fn persisted_plan_json(&self) -> AppResult<String> {
        Ok(canonical_json(&payload_value(&self.payload)))
    }

    /// Rehydrate a stored plan and recompute its canonical identity before execution.
    pub(crate) fn from_persisted_plan_json(plan_json: &str) -> AppResult<Self> {
        let value: Value = serde_json::from_str(plan_json)
            .map_err(|_| AppError::run(SafeRunErrorCode::InvalidChangePlan))?;
        if value.get("schemaVersion").and_then(Value::as_u64) == Some(2) {
            return Self::freeze_set(FrozenChangeSetInput {
                confirmation_id: required_string(&value, "confirmationId")?,
                run_id: required_string(&value, "runId")?,
                session_id: required_session_id(&value)?,
                request_id: required_string(&value, "requestId")?,
                vault_id: required_string(&value, "vaultId")?,
                operations: required_operations(&value)?,
                expires_at_unix_ms: required_expiry(&value)?,
            });
        }
        Self::freeze(FrozenChangePlanInput {
            confirmation_id: required_string(&value, "confirmationId")?,
            run_id: required_string(&value, "runId")?,
            session_id: required_session_id(&value)?,
            request_id: required_string(&value, "requestId")?,
            tool_call_id: required_string(&value, "toolCallId")?,
            vault_id: required_string(&value, "vaultId")?,
            relative_paths: required_string_array(&value, "relativePaths")?,
            operation: required_string(&value, "operation")?,
            base_content_hashes: required_hash_pairs(&value, "baseContentHashes")?,
            expected_post_content_hashes: required_hash_pairs(&value, "expectedPostContentHashes")?,
            change: required_change(&value)?,
            affected_file_count: required_affected_file_count(&value)?,
            rollback_summary: required_string(&value, "rollbackSummary")?,
            expires_at_unix_ms: required_expiry(&value)?,
        })
    }

    #[cfg(test)]
    pub(crate) fn validate_approval(
        &self,
        confirmation_id: &str,
        plan_hash: &str,
        now_unix_ms: i64,
    ) -> AppResult<()> {
        if confirmation_id != self.confirmation_id()
            || plan_hash != self.plan_hash
            || now_unix_ms > self.expires_at_unix_ms()
        {
            return Err(AppError::run(SafeRunErrorCode::ConfirmationExpired));
        }
        Ok(())
    }

    /// Validate a plan that was already consumed while it was unexpired.
    pub(crate) fn validate_consumed_identity(
        &self,
        confirmation_id: &str,
        plan_hash: &str,
    ) -> AppResult<()> {
        if confirmation_id != self.confirmation_id() || plan_hash != self.plan_hash {
            return Err(AppError::run(SafeRunErrorCode::ConfirmationExpired));
        }
        Ok(())
    }

    /// Reject execution when the live vault is no longer the frozen identity.
    pub(crate) fn assert_live_vault(&self, vault: &std::path::Path) -> AppResult<()> {
        assert_live_vault_id(vault, self.vault_id())
    }

    /// Ordered operations are the only authority for batch execution and recovery.
    pub(crate) fn operations(&self) -> &[FrozenChangeOperation] {
        &self.operations
    }
    /// Targets affected by the exact frozen set, deduplicated in first-use order.
    pub(crate) fn relative_paths(&self) -> &[String] {
        &self.relative_paths
    }
    pub(crate) fn all_base_content_hashes(&self) -> Vec<(String, String)> {
        self.operations
            .iter()
            .flat_map(|o| o.base_content_hashes().iter().cloned())
            .collect()
    }
    pub(crate) fn all_expected_post_content_hashes(&self) -> Vec<(String, String)> {
        self.operations
            .iter()
            .flat_map(|o| o.expected_post_content_hashes().iter().cloned())
            .collect()
    }
}

/// Outcome of revalidating disk content while Durable Apply is `Dispatching`.
///
/// `Dispatching` means the write receipt is unknown. Matching `expected_post`
/// means the write landed; matching `base` means it has not; anything else
/// must fail closed and must not be retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrozenWriteReceipt {
    NotYetApplied,
    AlreadyApplied,
    Unknown,
}

/// Classify one target's current hash against the frozen base and expected post hashes.
pub(crate) fn classify_frozen_write_receipt(
    current_hash: &str,
    base_hash: &str,
    expected_post_hash: &str,
) -> FrozenWriteReceipt {
    if current_hash == expected_post_hash {
        FrozenWriteReceipt::AlreadyApplied
    } else if current_hash == base_hash {
        FrozenWriteReceipt::NotYetApplied
    } else {
        FrozenWriteReceipt::Unknown
    }
}

/// Classify every target of one frozen operation. Mixed receipts are unknown.
pub(crate) fn classify_frozen_operation_receipt(
    current_by_path: &BTreeMap<String, String>,
    operation: &FrozenChangeOperation,
) -> FrozenWriteReceipt {
    let expected: BTreeMap<_, _> = operation
        .expected_post_content_hashes()
        .iter()
        .cloned()
        .collect();
    let mut seen = None;
    for (path, base_hash) in operation.base_content_hashes() {
        let Some(current) = current_by_path.get(path) else {
            return FrozenWriteReceipt::Unknown;
        };
        let Some(expected_hash) = expected.get(path) else {
            return FrozenWriteReceipt::Unknown;
        };
        let receipt = classify_frozen_write_receipt(current, base_hash, expected_hash);
        match seen {
            None => seen = Some(receipt),
            Some(previous) if previous == receipt => {}
            Some(_) => return FrozenWriteReceipt::Unknown,
        }
    }
    seen.unwrap_or(FrozenWriteReceipt::Unknown)
}

/// Read live files for one operation and classify the in-flight write receipt.
pub(crate) fn classify_operation_disk_receipt(
    vault: &std::path::Path,
    operation: &FrozenChangeOperation,
) -> FrozenWriteReceipt {
    let mut current = BTreeMap::new();
    for (path, _) in operation.base_content_hashes() {
        if path.starts_with("application://") {
            return FrozenWriteReceipt::Unknown;
        }
        let Ok(resolved) = crate::storage::paths::resolve_vault_path(vault, path) else {
            return FrozenWriteReceipt::Unknown;
        };
        let Ok(body) = std::fs::read_to_string(resolved) else {
            return FrozenWriteReceipt::Unknown;
        };
        current.insert(path.clone(), crate::cas::hash::content_hash_str(&body));
    }
    classify_frozen_operation_receipt(&current, operation)
}

/// Record a landed write when `Dispatching` is retried after the expected hash is already on disk.
pub(crate) fn recovered_write_receipt_result(
    tool_name: &str,
    operation: &FrozenChangeOperation,
) -> ToolCallResult {
    ToolCallResult {
        tool_name: tool_name.to_string(),
        success: true,
        output: serde_json::json!({
            "tool_call_id": operation.tool_call_id(),
            "recovered_write_receipt": true,
        }),
        duration_ms: 0,
        tokens_used: None,
        error: None,
    }
}

/// Align recovered suffix results to frozen operations by index and tool_call_id.
///
/// The executor only returns `skip(start)` results. Prefix operations already
/// applied at the checkpoint must stay successful and a later failure must not
/// be pasted onto operation 0.
pub(crate) fn merge_confirmed_results(
    plan: &FrozenChangePlan,
    start: usize,
    suffix: &[ToolCallResult],
) -> Vec<ToolCallResult> {
    plan.operations()
        .iter()
        .enumerate()
        .map(|(index, operation)| {
            if index < start {
                return ToolCallResult {
                    tool_name: operation.operation().to_string(),
                    success: true,
                    output: serde_json::json!({
                        "tool_call_id": operation.tool_call_id(),
                        "recovered_prefix": true,
                    }),
                    duration_ms: 0,
                    tokens_used: None,
                    error: None,
                };
            }
            let suffix_index = index.saturating_sub(start);
            if let Some(result) = suffix.get(suffix_index) {
                let id = result.output.get("tool_call_id").and_then(Value::as_str);
                if id.is_none_or(|id| id == operation.tool_call_id()) {
                    return result.clone();
                }
            }
            suffix
                .iter()
                .find(|result| {
                    result.output.get("tool_call_id").and_then(Value::as_str)
                        == Some(operation.tool_call_id())
                })
                .cloned()
                .unwrap_or_else(|| ToolCallResult {
                    tool_name: operation.operation().to_string(),
                    success: false,
                    output: serde_json::json!({
                        "error": "confirmed_operation_not_executed",
                        "tool_call_id": operation.tool_call_id(),
                    }),
                    duration_ms: 0,
                    tokens_used: None,
                    error: Some("confirmed_operation_not_executed".into()),
                })
        })
        .collect()
}

fn validate_legacy_input(input: &FrozenChangePlanInput) -> AppResult<()> {
    let operation = FrozenChangeOperationInput {
        tool_call_id: input.tool_call_id.clone(),
        operation: input.operation.clone(),
        relative_paths: input.relative_paths.clone(),
        base_content_hashes: input.base_content_hashes.clone(),
        expected_post_content_hashes: input.expected_post_content_hashes.clone(),
        change: input.change.clone(),
        rollback_summary: input.rollback_summary.clone(),
    };
    if input.confirmation_id.trim().is_empty()
        || input.run_id.trim().is_empty()
        || input.request_id.trim().is_empty()
        || input.vault_id.trim().is_empty()
        || input.affected_file_count != input.relative_paths.len()
    {
        return Err(AppError::run(SafeRunErrorCode::InvalidChangePlan));
    }
    validate_operation(&operation)
}

fn validate_operation(operation: &FrozenChangeOperationInput) -> AppResult<()> {
    let hashes_required = matches!(
        operation.operation.as_str(),
        "insert_text_at_cursor" | "replace_selection"
    );
    if operation.tool_call_id.trim().is_empty()
        || operation.operation.trim().is_empty()
        || operation.relative_paths.is_empty()
        || operation.relative_paths.len() > MAX_CHANGE_TARGETS
        || !hash_pairs_match_paths(
            &operation.base_content_hashes,
            &operation.relative_paths,
            hashes_required,
        )
        || !hash_pairs_match_paths(
            &operation.expected_post_content_hashes,
            &operation.relative_paths,
            hashes_required,
        )
        || !operation.change.is_object()
        || operation.rollback_summary.trim().is_empty()
    {
        return Err(AppError::run(SafeRunErrorCode::InvalidChangePlan));
    }
    Ok(())
}

fn unique_relative_paths(operations: &[FrozenChangeOperation]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    operations
        .iter()
        .flat_map(|operation| operation.relative_paths())
        .filter(|path| seen.insert((*path).clone()))
        .cloned()
        .collect()
}

fn required_string(value: &Value, field: &str) -> AppResult<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|item| !item.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidChangePlan))
}
fn required_session_id(value: &Value) -> AppResult<i64> {
    value
        .get("sessionId")
        .and_then(Value::as_i64)
        .filter(|id| *id > 0)
        .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidChangePlan))
}
fn required_expiry(value: &Value) -> AppResult<i64> {
    value
        .get("expiresAtUnixMs")
        .and_then(Value::as_i64)
        .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidChangePlan))
}
fn required_affected_file_count(value: &Value) -> AppResult<usize> {
    value
        .get("affectedFileCount")
        .and_then(Value::as_u64)
        .and_then(|count| usize::try_from(count).ok())
        .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidChangePlan))
}
fn required_change(value: &Value) -> AppResult<Value> {
    value
        .get("change")
        .cloned()
        .filter(Value::is_object)
        .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidChangePlan))
}

fn required_string_array(value: &Value, field: &str) -> AppResult<Vec<String>> {
    value
        .get(field)
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
        .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidChangePlan))?
        .iter()
        .map(|item| {
            item.as_str()
                .filter(|item| !item.trim().is_empty())
                .map(str::to_owned)
                .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidChangePlan))
        })
        .collect()
}

fn required_hash_pairs(value: &Value, field: &str) -> AppResult<Vec<(String, String)>> {
    value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidChangePlan))?
        .iter()
        .map(|pair| {
            let pair = pair
                .as_array()
                .filter(|pair| pair.len() == 2)
                .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidChangePlan))?;
            let path = pair[0]
                .as_str()
                .filter(|path| !path.trim().is_empty())
                .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidChangePlan))?;
            let hash = pair[1]
                .as_str()
                .filter(|hash| !hash.trim().is_empty())
                .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidChangePlan))?;
            Ok((path.to_owned(), hash.to_owned()))
        })
        .collect()
}

fn required_operations(value: &Value) -> AppResult<Vec<FrozenChangeOperationInput>> {
    value
        .get("operations")
        .and_then(Value::as_array)
        .filter(|operations| !operations.is_empty())
        .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidChangePlan))?
        .iter()
        .map(|operation| {
            Ok(FrozenChangeOperationInput {
                tool_call_id: required_string(operation, "toolCallId")?,
                operation: required_string(operation, "operation")?,
                relative_paths: required_string_array(operation, "relativePaths")?,
                base_content_hashes: required_hash_pairs(operation, "baseContentHashes")?,
                expected_post_content_hashes: required_hash_pairs(
                    operation,
                    "expectedPostContentHashes",
                )?,
                change: required_change(operation)?,
                rollback_summary: required_string(operation, "rollbackSummary")?,
            })
        })
        .collect()
}

fn hash_pairs_match_paths(pairs: &[(String, String)], paths: &[String], required: bool) -> bool {
    (!required && pairs.is_empty())
        || (pairs.len() == paths.len()
            && pairs
                .iter()
                .zip(paths)
                .all(|((path, hash), expected_path)| {
                    path == expected_path && !hash.trim().is_empty()
                }))
}

fn payload_value(payload: &FrozenChangePayload) -> Value {
    match payload {
        FrozenChangePayload::Legacy(input) => legacy_plan_value(input),
        FrozenChangePayload::Set(input) => serde_json::json!({
            "schemaVersion": 2, "confirmationId": input.confirmation_id, "runId": input.run_id, "sessionId": input.session_id, "requestId": input.request_id, "vaultId": input.vault_id,
            "operations": input.operations.iter().map(operation_value).collect::<Vec<_>>(), "affectedFileCount": unique_input_paths(&input.operations).len(), "expiresAtUnixMs": input.expires_at_unix_ms,
        }),
    }
}

fn legacy_plan_value(input: &FrozenChangePlanInput) -> Value {
    serde_json::json!({ "confirmationId": input.confirmation_id, "runId": input.run_id, "sessionId": input.session_id, "requestId": input.request_id, "toolCallId": input.tool_call_id, "vaultId": input.vault_id, "relativePaths": input.relative_paths, "operation": input.operation, "baseContentHashes": input.base_content_hashes, "expectedPostContentHashes": input.expected_post_content_hashes, "change": input.change, "affectedFileCount": input.affected_file_count, "rollbackSummary": input.rollback_summary, "expiresAtUnixMs": input.expires_at_unix_ms })
}
fn operation_value(input: &FrozenChangeOperationInput) -> Value {
    serde_json::json!({ "toolCallId": input.tool_call_id, "operation": input.operation, "relativePaths": input.relative_paths, "baseContentHashes": input.base_content_hashes, "expectedPostContentHashes": input.expected_post_content_hashes, "change": input.change, "rollbackSummary": input.rollback_summary })
}
/// Hash identity used to bind a frozen confirmation to one live vault.
pub(crate) fn live_vault_id(vault: &std::path::Path) -> String {
    crate::cas::hash::content_hash_str(&vault.to_string_lossy())
}

/// Fail closed when a confirmed write would land in a different vault.
pub(crate) fn assert_live_vault_id(
    vault: &std::path::Path,
    expected_vault_id: &str,
) -> AppResult<()> {
    if live_vault_id(vault) != expected_vault_id {
        return Err(AppError::run(SafeRunErrorCode::ConfirmationExpired));
    }
    Ok(())
}

fn unique_input_paths(operations: &[FrozenChangeOperationInput]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    operations
        .iter()
        .flat_map(|operation| &operation.relative_paths)
        .filter(|path| seen.insert((*path).clone()))
        .cloned()
        .collect()
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let ordered = map.iter().collect::<BTreeMap<_, _>>();
            let body = ordered
                .into_iter()
                .map(|(key, value)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap_or_default(),
                        canonical_json(value)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{body}}}")
        }
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}
