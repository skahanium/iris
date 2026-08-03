use super::*;

impl RunEngine {
    /// Convert unfinished work left by a previous process into a replayable safe state.
    /// Recover non-terminal normal Runs after a process restart.
    ///
    /// Direct and ToolLoop work always fails closed because its provider stream
    /// is gone. A consumed Durable Apply plan is classified only after current
    /// policy, target and content hashes are revalidated.
    pub(crate) fn recover_interrupted_runs(db: &Database) -> AppResult<usize> {
        let interrupted = db.with_read_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT r.run_id, r.status, r.state_version, r.effort, r.effect, s.session_key
                 FROM agent_runs r
                 JOIN sessions s ON s.id = r.session_id
                 WHERE r.status IN
                   ('accepted', 'preparing', 'running', 'verifying', 'awaiting_confirmation')",
            )?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into);
            rows
        })?;
        let mut recovered = 0;
        for (run_id, status, state_version, effort, effect, session_key) in interrupted {
            let state = serde_json::from_value::<RunState>(serde_json::Value::String(status))?;
            let effort = serde_json::from_value::<Effort>(serde_json::Value::String(effort))?;
            let effect = serde_json::from_value::<Effect>(serde_json::Value::String(effect))?;
            let confirmation = latest_confirmation_for_recovery(db, &run_id)?;

            if state == RunState::AwaitingConfirmation
                && confirmation
                    .as_ref()
                    .is_some_and(|confirmation| confirmation.status == "pending")
            {
                continue;
            }

            if confirmation
                .as_ref()
                .is_some_and(|confirmation| confirmation.status == "rejected")
            {
                AgentRunRepository::finalize(
                    db,
                    FinalizeRunInput {
                        run_id: run_id.clone(),
                        state_version,
                        content: "已取消该变更，未作任何修改。".into(),
                        evidence_ids: Vec::new(),
                        citation_map: serde_json::json!({}),
                        source_summary: Vec::new(),
                    },
                )?;
                recovered += 1;
                continue;
            }

            let consumed = confirmation
                .filter(|confirmation| confirmation.status == "consumed")
                .filter(|_| effort == Effort::Durable && effect == Effect::Apply);
            let Some(consumed) = consumed else {
                fail_interrupted_run(db, &run_id, state, state_version)?;
                recovered += 1;
                continue;
            };
            let plan =
                crate::ai_runtime::frozen_change_plan::FrozenChangePlan::from_persisted_plan_json(
                    &consumed.plan_json,
                );
            let classification = plan
                .as_ref()
                .ok()
                .filter(|plan| {
                    plan.plan_hash() == consumed.plan_hash
                        && plan.confirmation_id() == consumed.confirmation_id
                        && plan.run_id() == run_id
                })
                .and_then(|plan| {
                    classify_consumed_durable_apply(db, &session_key, &run_id, plan).ok()
                })
                .unwrap_or(DurableRecoveryClassification::ManualReview);

            match classification {
                DurableRecoveryClassification::ResumeAvailable => {
                    AgentRunRepository::append_event(
                        db,
                        AppendRunEventInput {
                            run_id: run_id.clone(),
                            state_version,
                            event_type: RunEventType::Paused,
                            payload: RunEventPayload::Paused {
                                reason: "目标仍与已确认计划的基础版本一致，可安全继续".into(),
                                recovery: Some(RunRecoveryKind::ResumeAvailable),
                            },
                        },
                    )?;
                }
                DurableRecoveryClassification::AlreadyApplied => {
                    let Ok(plan) = plan else {
                        return Err(AppError::msg("agent_run_invalid_change_plan"));
                    };
                    append_recovered_tool_completed_if_needed(db, &run_id, state_version, &plan)?;
                    advance_recovered_checkpoint_to_completed(db, &run_id, state_version, &plan)?;
                    AgentRunRepository::finalize(
                        db,
                        FinalizeRunInput {
                            run_id: run_id.clone(),
                            state_version,
                            content: "已执行你确认的变更。".into(),
                            evidence_ids: Vec::new(),
                            citation_map: serde_json::json!({}),
                            source_summary: Vec::new(),
                        },
                    )?;
                }
                DurableRecoveryClassification::ManualReview => {
                    AgentRunRepository::append_event(
                        db,
                        AppendRunEventInput {
                            run_id: run_id.clone(),
                            state_version,
                            event_type: RunEventType::Paused,
                            payload: RunEventPayload::Paused {
                                reason: "目标状态与已确认计划不一致，需要手动检查，未自动重放"
                                    .into(),
                                recovery: Some(RunRecoveryKind::ManualReviewRequired),
                            },
                        },
                    )?;
                }
            }
            recovered += 1;
        }
        Ok(recovered)
    }
}

/// Owns the minimal direct Run lifecycle without legacy Harness state.
#[derive(Debug)]
struct RecoveryConfirmation {
    confirmation_id: String,
    plan_hash: String,
    plan_json: String,
    status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DurableRecoveryClassification {
    ResumeAvailable,
    AlreadyApplied,
    ManualReview,
}

fn latest_confirmation_for_recovery(
    db: &Database,
    run_id: &str,
) -> AppResult<Option<RecoveryConfirmation>> {
    db.with_read_conn(|conn| {
        conn.query_row(
            "SELECT confirmation_id, plan_hash, plan_json, status
             FROM agent_run_confirmations
             WHERE run_id = ?1
             ORDER BY created_at DESC
             LIMIT 1",
            [run_id],
            |row| {
                Ok(RecoveryConfirmation {
                    confirmation_id: row.get(0)?,
                    plan_hash: row.get(1)?,
                    plan_json: row.get(2)?,
                    status: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    })
}

fn fail_interrupted_run(
    db: &Database,
    run_id: &str,
    state: RunState,
    state_version: u64,
) -> AppResult<()> {
    let failed_from_version = if state == RunState::Accepted {
        AgentRunRepository::append_event(
            db,
            AppendRunEventInput {
                run_id: run_id.to_string(),
                state_version,
                event_type: RunEventType::StageChanged,
                payload: RunEventPayload::StageChanged {
                    state: RunState::Preparing,
                    stage: "正在恢复运行状态".into(),
                    stage_code: Some(RunStageCode::Recovering),
                },
            },
        )?
        .state_version()
    } else {
        state_version
    };
    AgentRunRepository::append_event(
        db,
        AppendRunEventInput {
            run_id: run_id.to_string(),
            state_version: failed_from_version,
            event_type: RunEventType::Failed,
            payload: RunEventPayload::Failed {
                code: SafeRunErrorCode::PersistenceFailed,
                message: "运行因应用关闭而中断，请重新提交请求".into(),
            },
        },
    )?;
    Ok(())
}

fn classify_consumed_durable_apply(
    db: &Database,
    session_key: &str,
    run_id: &str,
    plan: &crate::ai_runtime::frozen_change_plan::FrozenChangePlan,
) -> AppResult<DurableRecoveryClassification> {
    plan.validate_consumed_identity(plan.confirmation_id(), plan.plan_hash())?;
    let request = AgentRunRepository::policy_request_for_session(db, session_key, run_id)?
        .ok_or_else(|| AppError::msg("agent_run_not_found"))?;
    if request.envelope.effect != Effect::Apply || request.envelope.effort != Effort::Durable {
        return Ok(DurableRecoveryClassification::ManualReview);
    }
    let decision = crate::ai_runtime::document_policy_repository::load_policy_decision_engine(db)?
        .evaluate_run(request);
    if decision.denial_code.is_some() {
        return Ok(DurableRecoveryClassification::ManualReview);
    }
    let Some(entry) = crate::ai_runtime::tool_catalog::catalog_find(plan.operation()) else {
        return Ok(DurableRecoveryClassification::ManualReview);
    };
    if !(entry.requires_confirmation
        && entry.implementation
            == crate::ai_runtime::tool_catalog::ToolImplementationStatus::Dispatchable)
    {
        return Ok(DurableRecoveryClassification::ManualReview);
    }
    let gate = crate::ai_runtime::tool_execution_pipeline::evaluate_tool_execution(
        db,
        crate::ai_runtime::tool_execution_pipeline::ToolExecutionGate {
            run_id,
            session_id: Some(plan.session_id()),
            run_step: 1,
            entry,
            args: plan.change(),
            authorized_capabilities: &decision.allowed_capabilities,
            skill_id: None,
            subagent_depth: 0,
        },
    )?;
    if gate.tool_result.is_some() {
        return Ok(DurableRecoveryClassification::ManualReview);
    }
    let write_target = recovered_write_target_path(db, session_key, run_id)?;
    let mut change_paths = recovered_change_paths(plan.change());
    if change_paths.is_empty()
        && matches!(
            plan.operation(),
            "insert_text_at_cursor" | "replace_selection"
        )
    {
        if let Some(write_target) = &write_target {
            change_paths.push(write_target.clone());
        }
    }
    if change_paths != plan.relative_paths()
        || write_target.as_deref() != plan.relative_paths().first().map(String::as_str)
    {
        return Ok(DurableRecoveryClassification::ManualReview);
    }
    let vault = load_recovery_vault_path(db)?
        .ok_or_else(|| AppError::msg("agent_run_recovery_vault_unavailable"))?;
    if crate::cas::hash::content_hash_str(&vault.to_string_lossy()) != plan.vault_id() {
        return Ok(DurableRecoveryClassification::ManualReview);
    }
    let checkpoint = AgentRunRepository::latest_durable_apply_checkpoint(db, run_id)?;
    let Some(checkpoint) = checkpoint else {
        return Ok(DurableRecoveryClassification::ManualReview);
    };
    if checkpoint.confirmation_id() != plan.confirmation_id()
        || checkpoint.plan_hash() != plan.plan_hash()
        || checkpoint.base_content_hashes()
            != plan
                .base_content_hashes()
                .iter()
                .map(|(_, hash)| hash.clone())
                .collect::<Vec<_>>()
        || checkpoint.expected_post_content_hashes()
            != plan
                .expected_post_content_hashes()
                .iter()
                .map(|(_, hash)| hash.clone())
                .collect::<Vec<_>>()
    {
        return Ok(DurableRecoveryClassification::ManualReview);
    }
    if plan.base_content_hashes().is_empty()
        || plan.base_content_hashes().len() != plan.expected_post_content_hashes().len()
    {
        return Ok(DurableRecoveryClassification::ManualReview);
    }
    let mut all_base = true;
    let mut all_expected = true;
    for ((path, base_hash), (expected_path, expected_hash)) in plan
        .base_content_hashes()
        .iter()
        .zip(plan.expected_post_content_hashes())
    {
        if path != expected_path || path.starts_with("application://") {
            return Ok(DurableRecoveryClassification::ManualReview);
        }
        let resolved = match crate::storage::paths::resolve_vault_path(&vault, path) {
            Ok(resolved) => resolved,
            Err(_) => return Ok(DurableRecoveryClassification::ManualReview),
        };
        let current = match std::fs::read_to_string(resolved) {
            Ok(current) => current,
            Err(_) => return Ok(DurableRecoveryClassification::ManualReview),
        };
        let current_hash = crate::cas::hash::content_hash_str(&current);
        all_base &= current_hash == *base_hash;
        all_expected &= current_hash == *expected_hash;
    }
    if all_expected {
        return Ok(DurableRecoveryClassification::AlreadyApplied);
    }
    if all_base
        && matches!(
            checkpoint.stage(),
            DurableApplyCheckpointStage::Approved | DurableApplyCheckpointStage::Dispatching
        )
    {
        return Ok(DurableRecoveryClassification::ResumeAvailable);
    }
    Ok(DurableRecoveryClassification::ManualReview)
}

fn recovered_change_paths(change: &serde_json::Value) -> Vec<String> {
    let mut paths = std::collections::BTreeSet::new();
    for key in ["target_path", "path", "new_path", "note_path"] {
        if let Some(path) = change
            .get(key)
            .and_then(serde_json::Value::as_str)
            .filter(|path| !path.trim().is_empty())
        {
            paths.insert(path.trim().replace('\\', "/"));
        }
    }
    paths.into_iter().collect()
}

fn recovered_write_target_path(
    db: &Database,
    session_key: &str,
    run_id: &str,
) -> AppResult<Option<String>> {
    db.with_read_conn(|conn| {
        let stored = conn
            .query_row(
                "SELECT r.explicit_action_json, m.explicit_references_json
                 FROM agent_runs r
                 JOIN sessions s ON s.id = r.session_id
                 JOIN session_messages m
                   ON m.session_id = r.session_id AND m.turn_id = r.turn_id AND m.role = 'user'
                 WHERE r.run_id = ?1 AND s.session_key = ?2",
                rusqlite::params![run_id, session_key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let Some((action_json, references_json)) = stored else {
            return Ok(None);
        };
        let action: serde_json::Value = serde_json::from_str(&action_json)?;
        let reference_id = action
            .get("target")
            .and_then(|target| target.get("referenceId"))
            .and_then(serde_json::Value::as_str);
        let Some(reference_id) = reference_id else {
            return Ok(None);
        };
        let references: Vec<serde_json::Value> = serde_json::from_str(&references_json)?;
        Ok(references
            .into_iter()
            .find(|reference| {
                reference.get("id").and_then(serde_json::Value::as_str) == Some(reference_id)
            })
            .and_then(|reference| {
                reference
                    .get("filePath")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            }))
    })
}

fn load_recovery_vault_path(db: &Database) -> AppResult<Option<std::path::PathBuf>> {
    db.with_read_conn(|conn| {
        let stored = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'vault_path'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        stored
            .map(|stored| {
                serde_json::from_str::<String>(&stored)
                    .map(std::path::PathBuf::from)
                    .map_err(Into::into)
            })
            .transpose()
    })
}

fn append_recovered_tool_completed_if_needed(
    db: &Database,
    run_id: &str,
    state_version: u64,
    plan: &crate::ai_runtime::frozen_change_plan::FrozenChangePlan,
) -> AppResult<()> {
    let existing = db.with_read_conn(|conn| {
        let mut statement = conn.prepare(
            "SELECT payload_json
             FROM agent_run_events
             WHERE run_id = ?1 AND event_type = 'tool_completed'
             ORDER BY event_seq",
        )?;
        let payloads = statement
            .query_map([run_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for payload_json in payloads {
            let payload = serde_json::from_str::<RunEventPayload>(&payload_json)?;
            if let RunEventPayload::ToolCompleted {
                capability,
                tool_call_id,
                success,
                ..
            } = payload
            {
                if tool_call_id == plan.tool_call_id() {
                    return Ok(Some((capability, success)));
                }
            }
        }
        Ok(None)
    })?;
    match existing {
        Some((capability, Some(true))) if capability == plan.operation() => return Ok(()),
        Some(_) => return Err(AppError::msg("agent_run_recovery_tool_lifecycle_conflict")),
        None => {}
    }
    AgentRunRepository::append_event(
        db,
        AppendRunEventInput {
            run_id: run_id.to_string(),
            state_version,
            event_type: RunEventType::ToolCompleted,
            payload: RunEventPayload::ToolCompleted {
                capability: plan.operation().to_string(),
                tool_call_id: plan.tool_call_id().to_string(),
                summary: "已恢复已确认的变更执行状态".into(),
                duration_ms: None,
                success: Some(true),
                subagent_batch_report: None,
            },
        },
    )?;
    Ok(())
}

fn advance_recovered_checkpoint_to_completed(
    db: &Database,
    run_id: &str,
    state_version: u64,
    plan: &crate::ai_runtime::frozen_change_plan::FrozenChangePlan,
) -> AppResult<()> {
    let latest = AgentRunRepository::latest_durable_apply_checkpoint(db, run_id)?
        .ok_or_else(|| AppError::msg("agent_run_checkpoint_stage_conflict"))?;
    let stages: &[DurableApplyCheckpointStage] = match latest.stage() {
        DurableApplyCheckpointStage::Approved => &[
            DurableApplyCheckpointStage::Dispatching,
            DurableApplyCheckpointStage::Applied,
            DurableApplyCheckpointStage::Completed,
        ],
        DurableApplyCheckpointStage::Dispatching => &[
            DurableApplyCheckpointStage::Applied,
            DurableApplyCheckpointStage::Completed,
        ],
        DurableApplyCheckpointStage::Applied => &[DurableApplyCheckpointStage::Completed],
        DurableApplyCheckpointStage::Completed => &[],
    };
    for stage in stages {
        AgentRunRepository::append_checkpoint_step(
            db,
            AppendRunCheckpointInput {
                run_id: run_id.to_string(),
                state_version,
                checkpoint: DurableApplyCheckpoint::new(
                    plan.confirmation_id(),
                    plan.plan_hash(),
                    *stage,
                    plan.base_content_hashes()
                        .iter()
                        .map(|(_, hash)| hash.clone())
                        .collect(),
                    plan.expected_post_content_hashes()
                        .iter()
                        .map(|(_, hash)| hash.clone())
                        .collect(),
                    Vec::new(),
                )?,
            },
        )?;
    }
    Ok(())
}
