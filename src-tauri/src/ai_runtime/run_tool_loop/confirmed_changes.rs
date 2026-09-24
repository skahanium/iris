//! Content-preserving candidate preparation and confirmed apply execution.

use super::*;
use crate::ai_runtime::content_preservation::{
    check_format_preservation, is_format_preservation_request, preservation_notice,
    unproven_tool_result,
};
use crate::ai_runtime::edit_candidate::{
    blocked_unprepared_candidate, prepare_edit_candidate, safe_candidate_summary,
    EditCandidateError,
};

impl NormalRunToolExecutor<'_> {
    pub(super) fn format_gate(
        &self,
        name: &str,
        args: &serde_json::Value,
        virtual_documents: &BTreeMap<String, String>,
    ) -> AppResult<Option<ToolCallResult>> {
        if !matches!(name, "replace_selection" | "insert_text_at_cursor") {
            return Ok(None);
        }
        let paths = frozen_relative_paths(name, args, self.context);
        let Some(path) = paths.first() else {
            return Ok(Some(blocked_unprepared_candidate(
                name,
                &EditCandidateError::MissingFields,
            )));
        };
        let (original, from_virtual) = self.write_tool_original(path, virtual_documents)?;
        let preview_args = preview_args_for_candidate(args, path, &original, from_virtual);
        let candidate = match prepare_edit_candidate(name, &preview_args, &original) {
            Ok(candidate) => candidate,
            Err(err) => return Ok(Some(blocked_unprepared_candidate(name, &err))),
        };
        if is_format_preservation_request(&self.context.user_message) {
            let report = check_format_preservation(&original, &candidate.candidate_body);
            // `failed` still refuses freeze. `unknown` (tables/HTML/ZWSP) enters
            // confirmation + diff; unapproved writes stay off the disk.
            if report.has_failed() {
                return Ok(Some(unproven_tool_result(name, &report)));
            }
        }
        Ok(None)
    }

    pub(super) fn confirmation_summary_for_plan(
        &self,
        entry: &crate::ai_runtime::tool_catalog::ToolCatalogEntry,
        args: &serde_json::Value,
        plan: &crate::ai_runtime::frozen_change_plan::FrozenChangePlan,
    ) -> (String, Option<serde_json::Value>) {
        if matches!(entry.name, "replace_selection" | "insert_text_at_cursor") {
            if let Some(path) = frozen_relative_paths(entry.name, args, self.context).first() {
                if let Ok((original, from_virtual)) =
                    self.write_tool_original(path, &BTreeMap::new())
                {
                    let preview_args =
                        preview_args_for_candidate(args, path, &original, from_virtual);
                    if let Ok(candidate) =
                        prepare_edit_candidate(entry.name, &preview_args, &original)
                    {
                        // K16 delivery of 「差异与不确定项」: `unknown` candidates
                        // freeze with a bounded uncertainty projection attached to
                        // the confirmation. `failed` never reaches this point (the
                        // pre-freeze gate blocks it); `proven` still reports its
                        // three `passed` states so the card is self-describing.
                        let format_preservation =
                            is_format_preservation_request(&self.context.user_message).then(|| {
                                preservation_notice(&check_format_preservation(
                                    &original,
                                    &candidate.candidate_body,
                                ))
                            });
                        return (safe_candidate_summary(&candidate), format_preservation);
                    }
                }
            }
        }
        (
            format!(
                "等待确认：{} 将修改 {} 个目标",
                entry.name,
                plan.relative_paths().len()
            ),
            None,
        )
    }

    fn write_tool_original(
        &self,
        path: &str,
        virtual_documents: &BTreeMap<String, String>,
    ) -> AppResult<(String, bool)> {
        if let Some(body) = virtual_documents.get(path) {
            return Ok((body.clone(), true));
        }
        let vault = self.state.vault_path()?;
        let resolved = crate::storage::paths::validate_user_note_relative_path(&vault, path)?;
        Ok((std::fs::read_to_string(resolved)?, false))
    }

    pub(super) fn freeze_change_plan(
        &self,
        call: &crate::ai_runtime::ToolCall,
        entry: &crate::ai_runtime::tool_catalog::ToolCatalogEntry,
        args: &serde_json::Value,
    ) -> AppResult<crate::ai_runtime::frozen_change_plan::FrozenChangePlan> {
        let operation = self.freeze_change_operation(call, entry, args, &mut BTreeMap::new())?;
        let vault_id = self
            .state
            .vault_path()
            .map(|vault| crate::cas::hash::content_hash_str(&vault.to_string_lossy()))
            .unwrap_or_else(|_| format!("normal-session:{}", self.context.session_id));
        crate::ai_runtime::frozen_change_plan::FrozenChangePlan::freeze_set(
            crate::ai_runtime::frozen_change_plan::FrozenChangeSetInput {
                confirmation_id: uuid::Uuid::new_v4().to_string(),
                run_id: self.accepted.run_id.clone(),
                session_id: self.context.session_id,
                request_id: self.accepted.run_id.clone(),
                vault_id,
                operations: vec![operation],
                expires_at_unix_ms: chrono::Utc::now().timestamp_millis()
                    + CHANGE_CONFIRMATION_TTL_MS,
            },
        )
    }

    pub(super) fn freeze_change_operation(
        &self,
        call: &crate::ai_runtime::ToolCall,
        entry: &crate::ai_runtime::tool_catalog::ToolCatalogEntry,
        args: &serde_json::Value,
        virtual_documents: &mut BTreeMap<String, String>,
    ) -> AppResult<crate::ai_runtime::frozen_change_plan::FrozenChangeOperationInput> {
        let relative_paths = frozen_relative_paths(entry.name, args, self.context);
        if matches!(entry.name, "replace_selection" | "insert_text_at_cursor") {
            let path = relative_paths
                .first()
                .cloned()
                .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidChangePlan))?;
            let (original, from_virtual) = self.write_tool_original(&path, virtual_documents)?;
            let preview_args = preview_args_for_candidate(args, &path, &original, from_virtual);
            let candidate = prepare_edit_candidate(entry.name, &preview_args, &original)
                .map_err(|_| AppError::run(SafeRunErrorCode::InvalidChangePlan))?;
            virtual_documents.insert(path.clone(), candidate.candidate_body.clone());
            let mut change = args.clone();
            change["base_content_hash"] = serde_json::json!(candidate.base_content_hash);
            return Ok(
                crate::ai_runtime::frozen_change_plan::FrozenChangeOperationInput {
                    tool_call_id: call.id.clone(),
                    relative_paths,
                    operation: entry.name.to_string(),
                    base_content_hashes: vec![(path.clone(), candidate.base_content_hash)],
                    expected_post_content_hashes: vec![(
                        path,
                        candidate.expected_post_content_hash,
                    )],
                    change,
                    rollback_summary: rollback_summary(entry.name),
                },
            );
        }
        let mut base_content_hashes =
            frozen_base_content_hashes(args, self.context, &relative_paths);
        let expected_post_content_hashes = expected_post_content_hashes(
            self.state.as_ref(),
            entry.name,
            args,
            &relative_paths,
            &mut base_content_hashes,
            virtual_documents,
        )?;
        Ok(
            crate::ai_runtime::frozen_change_plan::FrozenChangeOperationInput {
                tool_call_id: call.id.clone(),
                relative_paths,
                operation: entry.name.to_string(),
                base_content_hashes,
                expected_post_content_hashes,
                change: args.clone(),
                rollback_summary: rollback_summary(entry.name),
            },
        )
    }

    /// Execute each operation from one consumed change set in its frozen order.
    /// A failed or drifted operation stops the suffix; completed prefixes are
    /// preserved and reported to the caller rather than being silently retried.
    pub(crate) async fn execute_confirmed_frozen_change_set(
        &self,
        plan: &crate::ai_runtime::frozen_change_plan::FrozenChangePlan,
    ) -> AppResult<Vec<ToolCallResult>> {
        if plan.run_id() != self.accepted.run_id || plan.session_id() != self.context.session_id {
            return Err(AppError::run(SafeRunErrorCode::ConfirmationExpired));
        }
        plan.validate_consumed_identity(plan.confirmation_id(), plan.plan_hash())?;
        plan.assert_live_vault(&self.state.vault_path()?)?;
        AgentRunRepository::validate_durable_apply_checkpoint_binding(
            &self.state.db,
            &self.accepted.run_id,
            plan,
        )?;
        let snapshot = AgentRunRepository::get_for_session(
            &self.state.db,
            &self.accepted.session.session_key,
            &self.accepted.run_id,
        )?
        .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?;
        if snapshot.run.state != crate::ai_runtime::run_contract::RunState::Running {
            return Err(AppError::run(SafeRunErrorCode::IllegalTransition));
        }
        let checkpoint = AgentRunRepository::latest_durable_apply_checkpoint(
            &self.state.db,
            &self.accepted.run_id,
        )?
        .ok_or_else(|| AppError::run(SafeRunErrorCode::ConfirmationExpired))?;
        let start = checkpoint.next_operation_index();
        // Applied is persisted before ToolCompleted. A crash in that gap must
        // restore the original call's receipt without dispatching it again.
        for operation in plan.operations().iter().take(start) {
            let receipt = snapshot
                .events
                .iter()
                .find_map(|event| match event.payload() {
                    crate::ai_runtime::run_contract::RunEventPayload::ToolCompleted {
                        tool_call_id,
                        capability,
                        success,
                        ..
                    } if tool_call_id == operation.tool_call_id() => Some((capability, success)),
                    _ => None,
                });
            match receipt {
                Some((capability, Some(true))) if capability == operation.operation() => {}
                Some(_) => return Err(AppError::run(SafeRunErrorCode::ConfirmationExpired)),
                None => append_model_tool_completed(
                    &self.state.db,
                    self.accepted,
                    snapshot.run.state_version,
                    self.sink,
                    operation.operation(),
                    operation.tool_call_id(),
                    "已恢复已确认变更的执行回执",
                    0,
                    true,
                )?,
            }
        }
        let mut checkpoint_stage = checkpoint.stage();
        let mut results = Vec::new();
        for (index, operation) in plan.operations().iter().enumerate().skip(start) {
            let entry = catalog_find(operation.operation())
                .filter(|entry| entry.requires_confirmation
                    && entry.implementation == crate::ai_runtime::tool_catalog::ToolImplementationStatus::Dispatchable)
                .ok_or_else(|| AppError::run(SafeRunErrorCode::ConfirmationExpired))?;
            if frozen_relative_paths(entry.name, operation.change(), self.context)
                != operation.relative_paths()
            {
                return Err(AppError::run(SafeRunErrorCode::ConfirmationExpired));
            }
            let gate = ToolExecutionGate {
                run_id: &self.accepted.run_id,
                session_id: Some(self.context.session_id),
                run_step: u32::try_from(index + 1).unwrap_or(u32::MAX),
                entry,
                args: operation.change(),
                authorized_capabilities: &self.authorized_capabilities,
                skill_id: None,
                subagent_depth: 0,
            };
            let gate_outcome = evaluate_tool_execution(&self.state.db, gate)?;
            let result = if let Some(result) = gate_outcome.tool_result {
                result
            } else if checkpoint_stage == DurableApplyCheckpointStage::Dispatching {
                let receipt = self
                    .state
                    .vault_path()
                    .map(|vault| classify_operation_disk_receipt(&vault, operation))
                    .unwrap_or(FrozenWriteReceipt::Unknown);
                match receipt {
                    FrozenWriteReceipt::AlreadyApplied => {
                        self.append_durable_stage(
                            plan,
                            DurableApplyCheckpointStage::Applied,
                            index + 1,
                            snapshot.run.state_version,
                        )?;
                        checkpoint_stage = DurableApplyCheckpointStage::Applied;
                        recovered_write_receipt_result(entry.name, operation)
                    }
                    FrozenWriteReceipt::NotYetApplied => {
                        self.dispatch_frozen_and_checkpoint(
                            plan,
                            operation,
                            entry.name,
                            index,
                            snapshot.run.state_version,
                            &mut checkpoint_stage,
                        )
                        .await?
                    }
                    FrozenWriteReceipt::Unknown => {
                        failed_tool_call(entry.name, "frozen_change_write_receipt_unknown")
                    }
                }
            } else if revalidate_frozen_hash_pairs(
                self.state.as_ref(),
                operation.base_content_hashes(),
            )
            .is_err()
            {
                failed_tool_call(entry.name, "frozen_change_base_hash_drift")
            } else {
                self.append_durable_stage(
                    plan,
                    DurableApplyCheckpointStage::Dispatching,
                    index,
                    snapshot.run.state_version,
                )?;
                checkpoint_stage = DurableApplyCheckpointStage::Dispatching;
                self.dispatch_frozen_and_checkpoint(
                    plan,
                    operation,
                    entry.name,
                    index,
                    snapshot.run.state_version,
                    &mut checkpoint_stage,
                )
                .await?
            };
            audit_dispatched_tool(&self.state.db, &gate, &gate_outcome.decision, &result)?;
            append_model_tool_completed(
                &self.state.db,
                self.accepted,
                snapshot.run.state_version,
                self.sink,
                entry.name,
                operation.tool_call_id(),
                if result.success {
                    "已执行已确认的变更"
                } else {
                    "已确认的变更未执行"
                },
                result.duration_ms,
                result.success,
            )?;
            let succeeded = result.success;
            results.push(result);
            if !succeeded {
                break;
            }
        }
        Ok(crate::ai_runtime::frozen_change_plan::merge_confirmed_results(plan, start, &results))
    }

    fn append_durable_stage(
        &self,
        plan: &crate::ai_runtime::frozen_change_plan::FrozenChangePlan,
        stage: DurableApplyCheckpointStage,
        next_operation_index: usize,
        state_version: u64,
    ) -> AppResult<()> {
        AgentRunRepository::append_checkpoint_step(
            &self.state.db,
            AppendRunCheckpointInput {
                run_id: self.accepted.run_id.clone(),
                state_version,
                checkpoint: durable_apply_checkpoint(plan, stage, next_operation_index)?,
            },
        )
    }

    async fn dispatch_frozen_and_checkpoint(
        &self,
        plan: &crate::ai_runtime::frozen_change_plan::FrozenChangePlan,
        operation: &crate::ai_runtime::frozen_change_plan::FrozenChangeOperation,
        tool_name: &str,
        index: usize,
        state_version: u64,
        checkpoint_stage: &mut DurableApplyCheckpointStage,
    ) -> AppResult<ToolCallResult> {
        let result = self
            .dispatch_non_web_tool(
                tool_name,
                operation.change(),
                Some(plan.relative_paths()),
                Some(plan.vault_id()),
            )
            .await;
        if result.success {
            self.append_durable_stage(
                plan,
                DurableApplyCheckpointStage::Applied,
                index + 1,
                state_version,
            )?;
            *checkpoint_stage = DurableApplyCheckpointStage::Applied;
        }
        Ok(result)
    }
}

fn preview_args_for_candidate(
    args: &serde_json::Value,
    path: &str,
    original_body: &str,
    from_virtual: bool,
) -> serde_json::Value {
    let mut preview = args.clone();
    let has_path = ["target_path", "path"].iter().any(|key| {
        preview
            .get(*key)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
    });
    if !has_path {
        preview["target_path"] = serde_json::json!(path);
    }
    if from_virtual {
        preview["base_content_hash"] =
            serde_json::json!(crate::cas::hash::content_hash_str(original_body));
    }
    preview
}
