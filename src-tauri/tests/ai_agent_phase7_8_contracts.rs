use iris_lib::ai_runtime::subagent_coordinator::{
    ResourceAccess, ResourceLock, SubAgentCoordinator, SubAgentTaskSpec,
};
use iris_lib::storage::db::Database;

fn task(id: &str, access: ResourceAccess, note_path: &str) -> SubAgentTaskSpec {
    SubAgentTaskSpec {
        id: id.to_string(),
        role: "researcher".to_string(),
        task: format!("Inspect {note_path}"),
        allowed_tools: vec!["search_notes".to_string()],
        input_evidence_ids: vec!["ev-1".to_string()],
        output_schema: "SubagentReport".to_string(),
        resource_locks: vec![ResourceLock {
            resource_type: "note".to_string(),
            resource_id: note_path.to_string(),
            access,
        }],
        token_budget: Some(2048),
        failure_behavior: "report_error".to_string(),
    }
}

#[test]
fn subagent_coordinator_allows_concurrent_reads_and_blocks_same_note_writes() {
    let read_a = task("sub-a", ResourceAccess::Read, "Notes/Market.md");
    let read_b = task("sub-b", ResourceAccess::Read, "Notes/Market.md");

    let read_plan = SubAgentCoordinator::plan(&[read_a, read_b]);
    assert!(read_plan.can_run_concurrently);
    assert!(read_plan.conflicts.is_empty());

    let write_a = task("sub-c", ResourceAccess::Write, "Notes/Market.md");
    let write_b = task("sub-d", ResourceAccess::Write, "Notes/Market.md");

    let write_plan = SubAgentCoordinator::plan(&[write_a, write_b]);
    assert!(!write_plan.can_run_concurrently);
    assert_eq!(write_plan.conflicts.len(), 2);
    assert_eq!(write_plan.conflicts[0].resource_id, "Notes/Market.md");
}

#[test]
fn subagent_error_reports_do_not_become_parent_findings() {
    let spec = task("sub-error", ResourceAccess::Read, "Notes/Market.md");
    let report = SubAgentCoordinator::report_error(&spec, "child model failed");

    assert_eq!(report.subagent_id, "sub-error");
    assert!(report.findings.is_empty());
    assert_eq!(report.confidence, 0.0);
    assert_eq!(report.errors, vec!["child model failed".to_string()]);

    let parent_payload = SubAgentCoordinator::tool_output_for_report(&report);
    assert_eq!(
        parent_payload["subagent_report"]["subagent_id"],
        "sub-error"
    );
    assert!(parent_payload["subagent_report"]["findings"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn skill_trust_profiles_table_is_removed_by_reign_in() {
    let db = Database::open_in_memory().unwrap();

    let count: i64 = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'skill_trust_profiles'",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert_eq!(count, 0);
}
