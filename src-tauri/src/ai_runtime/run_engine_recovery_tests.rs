//! Checkpoint-boundary recovery regression tests.

use super::*;

#[test]
fn startup_recovery_completes_an_already_written_consumed_plan_without_replaying_it() {
    let (db, accepted, vault) = durable_apply_interrupted_after_consumed_confirmation();
    review_mark_confirmed_operation_dispatching(&db, &accepted);
    std::fs::write(vault.join("notes/a.md"), "after").expect("simulate committed write");

    assert_eq!(
        RunEngine::recover_interrupted_runs(&db).expect("recover written durable apply"),
        1
    );
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Completed);
    assert_eq!(
        AgentRunRepository::latest_durable_apply_checkpoint(&db, &accepted.run_id)
            .expect("checkpoint")
            .expect("completed checkpoint")
            .stage(),
        DurableApplyCheckpointStage::Completed
    );
    assert_eq!(
        std::fs::read_to_string(vault.join("notes/a.md")).expect("read recovered note"),
        "after"
    );
    let lifecycle = replay
        .events
        .iter()
        .filter_map(|event| {
            let event = serde_json::to_value(event).expect("serialize recovery event");
            matches!(
                event["type"].as_str(),
                Some("tool_started" | "confirmation_required" | "tool_completed" | "completed")
            )
            .then_some(event)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        lifecycle
            .iter()
            .map(|event| event["type"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        vec![
            "tool_started",
            "confirmation_required",
            "tool_completed",
            "completed"
        ]
    );
    let recovered_tool = &lifecycle[2]["payload"];
    assert_eq!(recovered_tool["capability"], "replace_selection");
    assert_eq!(
        recovered_tool["toolCallId"],
        format!("tool-{}", accepted.run_id)
    );
    assert_eq!(recovered_tool["summary"], "已恢复已确认的变更执行状态");
    assert_eq!(recovered_tool["success"], true);
    assert!(recovered_tool.get("arguments").is_none());
    assert!(recovered_tool.get("rawOutput").is_none());

    std::fs::remove_dir_all(vault).expect("remove recovery vault");
}

#[test]
fn startup_recovery_does_not_duplicate_an_already_recovered_tool_completion() {
    let (db, accepted, vault) = durable_apply_interrupted_after_consumed_confirmation();
    review_mark_confirmed_operation_dispatching(&db, &accepted);
    let state_version = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay before recovered completion")
        .expect("run")
        .run
        .state_version;
    AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version,
            event_type: RunEventType::ToolCompleted,
            payload: RunEventPayload::ToolCompleted {
                capability: "replace_selection".into(),
                tool_call_id: format!("tool-{}", accepted.run_id),
                summary: "已恢复已确认的变更执行状态".into(),
                duration_ms: None,
                success: Some(true),
                subagent_batch_report: None,
            },
        },
    )
    .expect("persist recovered completion before simulated crash");
    std::fs::write(vault.join("notes/a.md"), "after").expect("simulate committed write");

    assert_eq!(
        RunEngine::recover_interrupted_runs(&db).expect("resume interrupted recovery"),
        1
    );
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Completed);
    assert_eq!(
        replay
            .events
            .iter()
            .filter(|event| {
                serde_json::to_value(event).expect("serialize event")["type"] == "tool_completed"
            })
            .count(),
        1
    );

    std::fs::remove_dir_all(vault).expect("remove recovery vault");
}

fn review_mark_confirmed_operation_dispatching(
    db: &Database,
    accepted: &crate::ai_runtime::run_contract::AssistantRunAccepted,
) {
    let approved = AgentRunRepository::latest_durable_apply_checkpoint(db, &accepted.run_id)
        .expect("read approved checkpoint")
        .expect("approved checkpoint");
    assert_eq!(approved.stage(), DurableApplyCheckpointStage::Approved);
    let state_version = RunIntake::get(db, &accepted.session, &accepted.run_id)
        .expect("replay before dispatch")
        .expect("run")
        .run
        .state_version;
    AgentRunRepository::append_checkpoint_step(
        db,
        AppendRunCheckpointInput {
            run_id: accepted.run_id.clone(),
            state_version,
            checkpoint: DurableApplyCheckpoint::new_change_set(
                approved.confirmation_id(),
                approved.plan_hash(),
                DurableApplyCheckpointStage::Dispatching,
                approved.base_content_hashes().to_vec(),
                approved.expected_post_content_hashes().to_vec(),
                0,
                approved.operation_count(),
                Vec::new(),
            )
            .expect("dispatching checkpoint"),
        },
    )
    .expect("persist dispatch before simulated filesystem write");
}

#[test]
fn review_regression_c_approved_changed_disk_requires_manual_review() {
    let (db, accepted, vault) = durable_apply_interrupted_after_consumed_confirmation();
    std::fs::write(vault.join("notes/a.md"), "after")
        .expect("external writer happens to produce the proposed result");

    assert_eq!(
        RunEngine::recover_interrupted_runs(&db).expect("recover"),
        1
    );
    let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
        .expect("replay")
        .expect("run");
    assert_eq!(replay.run.state, RunState::Paused);
    assert_eq!(
        replay.run.recovery,
        Some(RunRecoveryKind::ManualReviewRequired)
    );
    assert!(replay.run.final_message_id.is_none());
    assert!(!replay
        .events
        .iter()
        .any(|event| matches!(event.payload(), RunEventPayload::ToolCompleted { .. })));
    assert_eq!(
        AgentRunRepository::latest_durable_apply_checkpoint(&db, &accepted.run_id)
            .expect("checkpoint")
            .expect("approved checkpoint")
            .stage(),
        DurableApplyCheckpointStage::Approved
    );
    assert_eq!(
        std::fs::read_to_string(vault.join("notes/a.md")).expect("disk"),
        "after"
    );
    std::fs::remove_dir_all(vault).expect("remove recovery vault");
}
