use super::frozen_change_plan::{
    FrozenChangeOperationInput, FrozenChangePlan, FrozenChangePlanInput, FrozenChangeSetInput,
};

fn input(diff: serde_json::Value) -> FrozenChangePlanInput {
    FrozenChangePlanInput {
        confirmation_id: "confirmation-1".to_string(),
        run_id: "run-1".to_string(),
        session_id: 42,
        request_id: "request-1".to_string(),
        tool_call_id: "tool-1".to_string(),
        vault_id: "vault-1".to_string(),
        relative_paths: vec!["notes/a.md".to_string()],
        operation: "note.apply_patch".to_string(),
        base_content_hashes: vec![("notes/a.md".to_string(), "hash-a".to_string())],
        expected_post_content_hashes: vec![("notes/a.md".to_string(), "hash-after".to_string())],
        change: diff,
        affected_file_count: 1,
        rollback_summary: "可通过版本历史撤销".to_string(),
        expires_at_unix_ms: i64::MAX,
    }
}

#[test]
fn frozen_plan_hash_is_canonical_and_rejects_any_changed_operation_or_diff() {
    let first = FrozenChangePlan::freeze(input(serde_json::json!({
        "replacement": "new", "range": { "end": 5, "start": 1 }
    })))
    .expect("freeze");
    let reordered = FrozenChangePlan::freeze(input(serde_json::json!({
        "range": { "start": 1, "end": 5 }, "replacement": "new"
    })))
    .expect("freeze reordered");
    assert_eq!(first.plan_hash(), reordered.plan_hash());
    assert!(first
        .validate_approval("confirmation-1", first.plan_hash(), 0)
        .is_ok());
    let changed = FrozenChangePlan::freeze(input(serde_json::json!({
        "range": { "start": 1, "end": 5 }, "replacement": "changed"
    })))
    .expect("freeze changed");
    assert_eq!(
        first
            .validate_approval("confirmation-1", changed.plan_hash(), 0)
            .unwrap_err()
            .to_string(),
        "agent_run_confirmation_expired"
    );
}

#[test]
fn expired_plan_cannot_be_approved() {
    let mut plan_input = input(serde_json::json!({ "replacement": "new" }));
    plan_input.expires_at_unix_ms = 0;
    let plan = FrozenChangePlan::freeze(plan_input).expect("freeze");

    assert_eq!(
        plan.validate_approval("confirmation-1", plan.plan_hash(), 1)
            .unwrap_err()
            .to_string(),
        "agent_run_confirmation_expired"
    );
    assert!(
        plan.validate_consumed_identity("confirmation-1", plan.plan_hash())
            .is_ok(),
        "consumed confirmation recovery must not recheck TTL"
    );
}

#[test]
fn persisted_plan_recomputes_the_hash_and_rejects_tampered_arguments() {
    let plan = FrozenChangePlan::freeze(input(serde_json::json!({
        "replacement": "approved"
    })))
    .expect("freeze");
    let persisted = plan.persisted_plan_json().expect("serialize plan");
    let restored = FrozenChangePlan::from_persisted_plan_json(&persisted).expect("restore plan");
    assert_eq!(restored.plan_hash(), plan.plan_hash());
    assert_eq!(restored.operations()[0].operation(), "note.apply_patch");
    assert_eq!(restored.operations()[0].change()["replacement"], "approved");
    assert_eq!(
        restored.operations()[0].expected_post_content_hashes(),
        &[("notes/a.md".to_string(), "hash-after".to_string())]
    );

    let mut tampered: serde_json::Value = serde_json::from_str(&persisted).expect("plan json");
    tampered["expectedPostContentHashes"][0][1] = serde_json::json!("tampered");
    let tampered = FrozenChangePlan::from_persisted_plan_json(
        &serde_json::to_string(&tampered).expect("tampered json"),
    )
    .expect("parse tampered plan");
    assert_ne!(tampered.plan_hash(), plan.plan_hash());
}

fn operation(
    tool_call_id: &str,
    path: &str,
    base_hash: &str,
    expected_hash: &str,
) -> FrozenChangeOperationInput {
    FrozenChangeOperationInput {
        tool_call_id: tool_call_id.to_string(),
        operation: "insert_text_at_cursor".to_string(),
        relative_paths: vec![path.to_string()],
        base_content_hashes: vec![(path.to_string(), base_hash.to_string())],
        expected_post_content_hashes: vec![(path.to_string(), expected_hash.to_string())],
        change: serde_json::json!({"target_path": path, "text": "updated"}),
        rollback_summary: "可通过版本历史撤销".to_string(),
    }
}

fn set_input(operations: Vec<FrozenChangeOperationInput>) -> FrozenChangeSetInput {
    FrozenChangeSetInput {
        confirmation_id: "confirmation-set-1".to_string(),
        run_id: "run-set-1".to_string(),
        session_id: 42,
        request_id: "request-set-1".to_string(),
        vault_id: "vault-set-1".to_string(),
        operations,
        expires_at_unix_ms: i64::MAX,
    }
}

#[test]
fn frozen_change_set_preserves_operation_order_and_limits_operations_and_targets() {
    let plan = FrozenChangePlan::freeze_set(set_input(vec![
        operation("tool-1", "notes/a.md", "a0", "a1"),
        operation("tool-2", "notes/b.md", "b0", "b1"),
    ]))
    .expect("freeze ordered set");

    assert_eq!(plan.operations().len(), 2);
    assert_eq!(plan.operations()[0].tool_call_id(), "tool-1");
    assert_eq!(plan.operations()[1].tool_call_id(), "tool-2");
    assert_eq!(plan.relative_paths(), ["notes/a.md", "notes/b.md"]);

    let too_many_operations = (0..7)
        .map(|index| {
            operation(
                &format!("tool-{index}"),
                "notes/a.md",
                &format!("a{index}"),
                &format!("a{}", index + 1),
            )
        })
        .collect();
    assert_eq!(
        FrozenChangePlan::freeze_set(set_input(too_many_operations))
            .expect_err("seven operations must be rejected")
            .to_string(),
        "agent_run_invalid_change_plan"
    );

    let too_many_targets = vec![FrozenChangeOperationInput {
        tool_call_id: "too-many-targets".to_string(),
        operation: "insert_text_at_cursor".to_string(),
        relative_paths: (0..7).map(|index| format!("notes/{index}.md")).collect(),
        base_content_hashes: (0..7)
            .map(|index| (format!("notes/{index}.md"), format!("h{index}")))
            .collect(),
        expected_post_content_hashes: (0..7)
            .map(|index| (format!("notes/{index}.md"), format!("p{index}")))
            .collect(),
        change: serde_json::json!({"target_path": "notes/0.md", "text": "updated"}),
        rollback_summary: "可通过版本历史撤销".to_string(),
    }];
    assert_eq!(
        FrozenChangePlan::freeze_set(set_input(too_many_targets))
            .expect_err("seven files must be rejected")
            .to_string(),
        "agent_run_invalid_change_plan"
    );

    assert_eq!(
        FrozenChangePlan::freeze_set(set_input(vec![
            operation("duplicate", "notes/a.md", "a0", "a1"),
            operation("duplicate", "notes/b.md", "b0", "b1"),
        ]))
        .expect_err("duplicate tool call IDs must be rejected")
        .to_string(),
        "agent_run_invalid_change_plan"
    );
}

#[test]
fn frozen_change_set_requires_a_hash_chain_when_an_operation_revisits_a_target() {
    let chained = FrozenChangePlan::freeze_set(set_input(vec![
        operation("tool-1", "notes/a.md", "a0", "a1"),
        operation("tool-2", "notes/a.md", "a1", "a2"),
    ]));
    assert!(
        chained.is_ok(),
        "the second base hash must see the first output"
    );

    assert_eq!(
        FrozenChangePlan::freeze_set(set_input(vec![
            operation("tool-1", "notes/a.md", "a0", "a1"),
            operation("tool-2", "notes/a.md", "a0", "a2"),
        ]))
        .expect_err("stale second operation must be rejected")
        .to_string(),
        "agent_run_invalid_change_plan"
    );
}
