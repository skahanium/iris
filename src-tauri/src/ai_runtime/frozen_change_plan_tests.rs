use super::frozen_change_plan::{FrozenChangePlan, FrozenChangePlanInput};

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
}
