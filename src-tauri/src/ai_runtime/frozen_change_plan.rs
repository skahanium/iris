//! Immutable, hash-bound confirmation payloads for change effects.

use std::collections::BTreeMap;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

/// Inputs that must be frozen before a user can approve a change effect.
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
    pub(crate) change: Value,
    pub(crate) affected_file_count: usize,
    pub(crate) rollback_summary: String,
    pub(crate) expires_at_unix_ms: i64,
}

/// Frozen plan plus its canonical SHA-256 identity.
#[derive(Debug, Clone)]
pub(crate) struct FrozenChangePlan {
    input: FrozenChangePlanInput,
    plan_hash: String,
}

impl FrozenChangePlan {
    /// Validate and freeze an approval payload before it can reach dispatch.
    pub(crate) fn freeze(input: FrozenChangePlanInput) -> AppResult<Self> {
        if input.confirmation_id.trim().is_empty()
            || input.run_id.trim().is_empty()
            || input.request_id.trim().is_empty()
            || input.tool_call_id.trim().is_empty()
            || input.vault_id.trim().is_empty()
            || input.operation.trim().is_empty()
            || input.relative_paths.is_empty()
            || input.affected_file_count != input.relative_paths.len()
            || input.rollback_summary.trim().is_empty()
        {
            return Err(AppError::msg("agent_run_invalid_change_plan"));
        }
        let canonical = canonical_json(&plan_value(&input));
        let hash = Sha256::digest(canonical.as_bytes());
        Ok(Self {
            input,
            plan_hash: format!("sha256:{}", hex::encode(hash)),
        })
    }

    /// Stable hash shown to and returned by the user confirmation UI.
    pub(crate) fn plan_hash(&self) -> &str {
        &self.plan_hash
    }

    pub(crate) fn confirmation_id(&self) -> &str {
        &self.input.confirmation_id
    }

    pub(crate) fn run_id(&self) -> &str {
        &self.input.run_id
    }

    pub(crate) const fn session_id(&self) -> i64 {
        self.input.session_id
    }

    pub(crate) fn expires_at_unix_ms(&self) -> i64 {
        self.input.expires_at_unix_ms
    }

    pub(crate) fn persisted_plan_json(&self) -> AppResult<String> {
        Ok(canonical_json(&plan_value(&self.input)))
    }

    /// Rehydrate a stored plan and recompute its canonical identity before execution.
    ///
    /// The database is not trusted as an execution authority: callers must use
    /// this constructor after loading `plan_json` so an altered payload cannot be
    /// dispatched under a previously approved hash.
    pub(crate) fn from_persisted_plan_json(plan_json: &str) -> AppResult<Self> {
        let value: Value = serde_json::from_str(plan_json)
            .map_err(|_| AppError::msg("agent_run_invalid_change_plan"))?;
        let input = FrozenChangePlanInput {
            confirmation_id: required_string(&value, "confirmationId")?,
            run_id: required_string(&value, "runId")?,
            session_id: value
                .get("sessionId")
                .and_then(Value::as_i64)
                .filter(|session_id| *session_id > 0)
                .ok_or_else(|| AppError::msg("agent_run_invalid_change_plan"))?,
            request_id: required_string(&value, "requestId")?,
            tool_call_id: required_string(&value, "toolCallId")?,
            vault_id: required_string(&value, "vaultId")?,
            relative_paths: required_string_array(&value, "relativePaths")?,
            operation: required_string(&value, "operation")?,
            base_content_hashes: required_hash_pairs(&value)?,
            change: value
                .get("change")
                .cloned()
                .filter(Value::is_object)
                .ok_or_else(|| AppError::msg("agent_run_invalid_change_plan"))?,
            affected_file_count: value
                .get("affectedFileCount")
                .and_then(Value::as_u64)
                .and_then(|count| usize::try_from(count).ok())
                .ok_or_else(|| AppError::msg("agent_run_invalid_change_plan"))?,
            rollback_summary: required_string(&value, "rollbackSummary")?,
            expires_at_unix_ms: value
                .get("expiresAtUnixMs")
                .and_then(Value::as_i64)
                .ok_or_else(|| AppError::msg("agent_run_invalid_change_plan"))?,
        };
        Self::freeze(input)
    }

    /// Validate approval identity, exact plan hash, and expiry before dispatch.
    pub(crate) fn validate_approval(
        &self,
        confirmation_id: &str,
        plan_hash: &str,
        now_unix_ms: i64,
    ) -> AppResult<()> {
        if confirmation_id != self.input.confirmation_id
            || plan_hash != self.plan_hash
            || now_unix_ms > self.input.expires_at_unix_ms
        {
            return Err(AppError::msg("agent_run_confirmation_expired"));
        }
        Ok(())
    }

    /// Tool identity that must be dispatched after approval.
    pub(crate) fn operation(&self) -> &str {
        &self.input.operation
    }

    /// Exact model-produced arguments that were shown in the frozen plan.
    pub(crate) fn change(&self) -> &Value {
        &self.input.change
    }

    /// Original provider tool-call identifier; it binds the completion event.
    pub(crate) fn tool_call_id(&self) -> &str {
        &self.input.tool_call_id
    }

    /// Targets affected by the exact frozen operation.
    pub(crate) fn relative_paths(&self) -> &[String] {
        &self.input.relative_paths
    }

    /// Optimistic content identities captured before confirmation.
    pub(crate) fn base_content_hashes(&self) -> &[(String, String)] {
        &self.input.base_content_hashes
    }
}

fn required_string(value: &Value, field: &str) -> AppResult<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|item| !item.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| AppError::msg("agent_run_invalid_change_plan"))
}

fn required_string_array(value: &Value, field: &str) -> AppResult<Vec<String>> {
    value
        .get(field)
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
        .ok_or_else(|| AppError::msg("agent_run_invalid_change_plan"))?
        .iter()
        .map(|item| {
            item.as_str()
                .filter(|item| !item.trim().is_empty())
                .map(str::to_owned)
                .ok_or_else(|| AppError::msg("agent_run_invalid_change_plan"))
        })
        .collect()
}

fn required_hash_pairs(value: &Value) -> AppResult<Vec<(String, String)>> {
    let pairs = value
        .get("baseContentHashes")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::msg("agent_run_invalid_change_plan"))?;
    pairs
        .iter()
        .map(|pair| {
            let pair = pair
                .as_array()
                .filter(|pair| pair.len() == 2)
                .ok_or_else(|| AppError::msg("agent_run_invalid_change_plan"))?;
            let path = pair[0]
                .as_str()
                .filter(|path| !path.trim().is_empty())
                .ok_or_else(|| AppError::msg("agent_run_invalid_change_plan"))?;
            let hash = pair[1]
                .as_str()
                .filter(|hash| !hash.trim().is_empty())
                .ok_or_else(|| AppError::msg("agent_run_invalid_change_plan"))?;
            Ok((path.to_owned(), hash.to_owned()))
        })
        .collect()
}

fn plan_value(input: &FrozenChangePlanInput) -> Value {
    serde_json::json!({
        "confirmationId": input.confirmation_id,
        "runId": input.run_id,
        "sessionId": input.session_id,
        "requestId": input.request_id,
        "toolCallId": input.tool_call_id,
        "vaultId": input.vault_id,
        "relativePaths": input.relative_paths,
        "operation": input.operation,
        "baseContentHashes": input.base_content_hashes,
        "change": input.change,
        "affectedFileCount": input.affected_file_count,
        "rollbackSummary": input.rollback_summary,
        "expiresAtUnixMs": input.expires_at_unix_ms,
    })
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
