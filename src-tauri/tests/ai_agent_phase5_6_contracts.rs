use iris_lib::ai_runtime::research_state::{ResearchState, ResearchStateInput};
use iris_lib::ai_runtime::writing_state::{WritingState, WritingStateInput};
use iris_lib::ai_runtime::{
    ContextPacket, PatchProposal, RiskLevel, SourceSpan, SourceType, TrustLevel,
};

fn packet(
    id: &str,
    label: &str,
    trust_level: TrustLevel,
    source_type: SourceType,
) -> ContextPacket {
    ContextPacket {
        id: id.into(),
        source_type,
        source_path: Some(format!("sources/{id}.md")),
        title: format!("证据 {id}"),
        heading_path: None,
        source_span: None,
        content_hash: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
        excerpt: "行业研究证据摘要".into(),
        retrieval_reason: "phase contract".into(),
        score: 0.82,
        trust_level,
        citation_label: label.into(),
        stale: false,
        web: None,
        corpus: None,
    }
}

fn patch() -> PatchProposal {
    PatchProposal {
        id: "patch-phase5".into(),
        target_path: "Drafts/report.md".into(),
        base_content_hash: "basehash".into(),
        range: SourceSpan { start: 12, end: 48 },
        original_text: "旧段落".into(),
        replacement_text: "新段落 [S1]".into(),
        evidence_packet_ids: vec!["ev-1".into()],
        risk_level: RiskLevel::Medium,
        warnings: vec!["中风险补丁".into()],
        created_at: "2026-06-20T00:00:00".into(),
    }
}

#[test]
fn writing_state_tracks_document_goal_style_versions_and_revision_reasons() {
    let state = WritingState::from_input(WritingStateInput {
        request_id: "phase5-writing".into(),
        target_path: "Drafts/report.md".into(),
        base_content_hash: "basehash".into(),
        writing_goal:
            "为投资备忘录改写引言，受众: 投委会，体裁: 行业研究备忘录，风格: 克制、证据驱动".into(),
        intent: "rewrite".into(),
        evidence: vec![packet("ev-1", "S1", TrustLevel::UserNote, SourceType::Note)],
        patches: vec![patch()],
    });

    assert_eq!(state.request_id, "phase5-writing");
    assert_eq!(state.target_path, "Drafts/report.md");
    assert_eq!(state.draft_version_hash, "basehash");
    assert!(state.document_goal.contains("投资备忘录"));
    assert!(state.audience.contains("投委会"));
    assert!(state.genre.contains("行业研究备忘录"));
    assert!(state
        .style_constraints
        .iter()
        .any(|s| s.contains("证据驱动")));
    assert_eq!(state.material_packet_ids, vec!["ev-1"]);
    assert_eq!(state.citation_labels, vec!["S1"]);
    assert_eq!(state.revision_records.len(), 1);

    let record = &state.revision_records[0];
    assert_eq!(record.patch_id, "patch-phase5");
    assert!(record.scope.contains("12..48"));
    assert!(record.reason.contains("rewrite"));
    assert!(record.rollback.contains("basehash"));
    assert!(record.risk.contains("medium"));

    let json = serde_json::to_value(&state).unwrap();
    assert!(json.get("full_content").is_none());
    assert!(json.get("note_content").is_none());
    assert!(json.get("raw_selection").is_none());
}

#[test]
fn research_state_tracks_sources_credibility_freshness_conflicts_and_boundaries() {
    let local = packet("ev-local", "S1", TrustLevel::UserNote, SourceType::Note);
    let web = packet("ev-web", "W1", TrustLevel::ExternalWeb, SourceType::Web);
    let state = ResearchState::from_input(ResearchStateInput {
        request_id: "phase6-research".into(),
        topic: "AI agent 行业研究".into(),
        questions: vec![
            "P1: 需求侧是否持续增长".into(),
            "P2: 成本曲线是否改善".into(),
        ],
        evidence: vec![local, web],
        global_gaps: vec!["缺少 2026 年一手收入数据".into()],
        has_contradictions: true,
        summary: "初步结论: agent 市场增长，但商业化节奏仍需验证。".into(),
    });

    assert_eq!(state.request_id, "phase6-research");
    assert_eq!(state.research_question, "AI agent 行业研究");
    assert_eq!(state.sub_questions.len(), 2);
    assert_eq!(state.sources.len(), 2);
    assert!(state
        .sources
        .iter()
        .any(|source| source.evidence_id == "ev-local" && source.credibility == "high"));
    assert!(state
        .sources
        .iter()
        .any(|source| source.evidence_id == "ev-web" && source.freshness == "needs_check"));
    assert!(!state.conflicts.is_empty());
    assert!(!state.counter_arguments.is_empty());
    assert!(state.evidence_gaps[0].contains("一手收入数据"));
    assert!(state
        .preliminary_conclusions
        .iter()
        .any(|conclusion| conclusion.boundary.contains("需验证")));
    assert!(state
        .preliminary_conclusions
        .iter()
        .all(|conclusion| !conclusion.evidence_item_ids.is_empty() || conclusion.inference));

    let json = serde_json::to_value(&state).unwrap();
    assert!(json.get("raw_web_page").is_none());
    assert!(json.get("full_note_content").is_none());
}

#[test]
fn phase5_6_source_contracts_expose_states_without_direct_md_writes() {
    let writing_commands = include_str!("../src/commands/writing_commands.rs");
    let research_commands = include_str!("../src/commands/research_commands.rs");
    let writing_workflow = include_str!("../src/ai_workflows/writing_workflow.rs");
    let research_workflow = include_str!("../src/ai_workflows/research_workflow.rs");
    let research_state = include_str!("../src/ai_runtime/research_state.rs");

    assert!(writing_workflow.contains("pub writing_state: WritingState"));
    assert!(writing_commands.contains("WritingState::from_input"));
    assert!(writing_commands.contains("patch_apply"));
    assert!(writing_commands.contains("base_content_hash"));
    assert!(
        !writing_commands.contains("execute_writing_task")
            || !writing_commands.contains("file_write_inner(state, &input.target_path")
    );

    assert!(research_workflow.contains("pub research_state: ResearchState"));
    assert!(research_commands.contains("research_state: result.research_state"));
    assert!(research_workflow.contains("ResearchState::from_input"));
    assert!(research_state.contains("pub struct EvidenceItem"));
}
