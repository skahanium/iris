use std::collections::HashMap;

use iris_lib::ai_runtime::skill_trust_policy::{
    build_skill_trust_profile, evaluate_skill_trust, persist_skill_trust_profile, SkillSourceKind,
    SkillTrustRiskLevel,
};
use iris_lib::ai_runtime::skills::{SkillEntry, SkillScope};
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

fn skill(name: &str, allowed_tools: Vec<&str>, requested_capabilities: Vec<&str>) -> SkillEntry {
    let mut metadata = HashMap::new();
    metadata.insert(
        "requested-capabilities".to_string(),
        serde_json::Value::Array(
            requested_capabilities
                .into_iter()
                .map(|capability| serde_json::Value::String(capability.to_string()))
                .collect(),
        ),
    );
    SkillEntry {
        name: name.to_string(),
        description: "A test skill".to_string(),
        license: Some("MIT".to_string()),
        compatibility: Some("iris".to_string()),
        metadata,
        allowed_tools: allowed_tools.into_iter().map(str::to_string).collect(),
        content: "# Test skill".to_string(),
        scope: SkillScope::Vault,
        source_url: Some("https://example.com/skill.git".to_string()),
        enabled: true,
        file_path: "/tmp/SKILL.md".to_string(),
        legacy_trigger: None,
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
fn skill_trust_policy_blocks_high_risk_and_prevents_allowed_tools_escalation() {
    let entry = skill(
        "danger-skill",
        vec!["search_notes", "root_shell"],
        vec!["skill.execute_script_sandboxed"],
    );

    let profile = build_skill_trust_profile(
        &entry,
        SkillSourceKind::Git,
        Some("https://example.com/skill.git"),
        Some("abc123"),
        None,
    );
    let decision = evaluate_skill_trust(&profile);

    assert_eq!(profile.risk_level, SkillTrustRiskLevel::High);
    assert!(!profile.allowed_tools_narrowing_only);
    assert!(profile.high_risk);
    assert!(!decision.auto_activate);
    assert!(decision.requires_confirmation);
    assert!(decision
        .warnings
        .iter()
        .any(|warning| warning.contains("sha256")));
}

#[test]
fn skill_trust_profiles_persist_for_all_install_sources() {
    let db = Database::open_in_memory().unwrap();
    let entry = skill(
        "trusted-skill",
        vec!["search_notes"],
        vec!["skill.read_resource"],
    );

    for source in [
        SkillSourceKind::Registry,
        SkillSourceKind::Git,
        SkillSourceKind::Url,
        SkillSourceKind::Local,
    ] {
        let profile = build_skill_trust_profile(
            &entry,
            source,
            Some("https://example.com/skill"),
            Some("abc123"),
            Some("abc123"),
        );
        persist_skill_trust_profile(&db, &profile).unwrap();
    }

    let count: i64 = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(DISTINCT source_type) FROM skill_trust_profiles WHERE skill_name = ?1",
                ["trusted-skill"],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert_eq!(count, 4);
}
