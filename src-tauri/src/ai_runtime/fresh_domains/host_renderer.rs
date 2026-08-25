//! Host-owned deterministic rendering for validated current-fact DTOs.

use crate::ai_runtime::agent_evidence_repository::AgentEvidenceRepository;
use crate::ai_runtime::final_answer_submission::{FinalAnswerBlock, FinalAnswerSubmission};
use crate::ai_runtime::run_contract::FreshFactPolicy;
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

use super::contracts::FreshDomainRecord;

/// Render registered structured DTOs without asking the model to reproduce
/// provider fields or source identities. A missing/invalid DTO fails closed.
pub(crate) fn render_current_run_submission(
    db: &Database,
    run_id: &str,
    policy: &FreshFactPolicy,
) -> AppResult<Option<FinalAnswerSubmission>> {
    if policy.schema_version != 1 {
        return Err(AppError::msg("fresh_domain_host_renderer_policy_invalid"));
    }
    let excerpts = AgentEvidenceRepository::list_current_run_external_excerpts(db, run_id)?;
    if excerpts.is_empty() {
        return Ok(None);
    }
    let mut blocks = Vec::with_capacity(excerpts.len());
    for (evidence_id, excerpt) in excerpts {
        let record: FreshDomainRecord = serde_json::from_str(&excerpt)
            .map_err(|_| AppError::msg("fresh_domain_host_renderer_record_invalid"))?;
        let markdown = render_record(&record);
        if markdown.trim().is_empty() {
            return Err(AppError::msg("fresh_domain_host_renderer_record_empty"));
        }
        blocks.push(FinalAnswerBlock {
            markdown,
            sources: vec![format!("E{evidence_id}")],
        });
    }
    Ok(Some(FinalAnswerSubmission { blocks }))
}

fn render_record(record: &FreshDomainRecord) -> String {
    match record {
        FreshDomainRecord::Weather(value) => format!(
            "天气：{}，{}，{}{}（观测时间：{}）。",
            value.location,
            value.condition,
            value.temperature,
            value.units,
            value
                .observation_time
                .as_deref()
                .or(value.issue_time.as_deref())
                .unwrap_or("未提供")
        ),
        FreshDomainRecord::News(value) => format!(
            "新闻：{}；发布方：{}；发布时间：{}。",
            value.title, value.publisher, value.published_at
        ),
        FreshDomainRecord::Finance(value) => format!(
            "{}（{}）：{} {}；截至：{}；延迟：{}。",
            value.instrument,
            value.asset_kind,
            value.value,
            value.currency,
            value.as_of,
            value.delay
        ),
        FreshDomainRecord::Entertainment(value) => format!(
            "{}：{}；渠道：{}；日期：{}；核验时间：{}。",
            value.region, value.title, value.channel, value.date, value.checked_at
        ),
        FreshDomainRecord::Sports(value) => format!(
            "{}：{}；开始时间：{}；状态：{}{}；核验时间：{}。",
            value.competition,
            value.participants.join(" vs "),
            value.start_time,
            value.status,
            value
                .score
                .as_deref()
                .map(|score| format!("；比分：{}", score))
                .unwrap_or_default(),
            value.checked_at
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{render_current_run_submission, render_record};
    use crate::ai_runtime::agent_evidence_repository::{
        AgentEvidenceRepository, ExternalToolEvidenceInput,
    };
    use crate::ai_runtime::agent_run_repository::{AcceptRunInput, AgentRunRepository};
    use crate::ai_runtime::fresh_domains::contracts::{
        EntertainmentRecord, EvidenceOrigin, FreshDomainRecord,
    };
    use crate::ai_runtime::normal_session_repository::NormalSessionRepository;
    use crate::ai_runtime::provenance::validate_final_answer_submission;
    use crate::ai_runtime::run_contract::{
        ContextMode, Effect, Effort, ExecutionEnvelope, FreshFactDomain, FreshFactPolicy,
        Freshness, RiskClass, SecurityDomain, WebDecisionReason,
    };
    use crate::storage::db::Database;

    fn accepted_run_with_high_external_evidence_id() -> (Database, FreshFactPolicy, String) {
        let db = Database::open_in_memory().expect("database");
        let session = NormalSessionRepository::create(&db).expect("normal session");
        let run_id = "host-renderer-high-evidence".to_string();
        let policy = FreshFactPolicy {
            schema_version: 1,
            domain: FreshFactDomain::Entertainment,
            operation: None,
            window_start: None,
            window_end: None,
            location_requirement: Default::default(),
        };
        AgentRunRepository::accept(
            &db,
            AcceptRunInput {
                session_id: session.session_id,
                session_key: session.session_key,
                client_request_id: "host-renderer-client".into(),
                run_id: run_id.clone(),
                turn_id: "host-renderer-turn".into(),
                message: "查询结构化当前事实".into(),
                content_parts: None,
                explicit_references: vec![],
                context_scope: Default::default(),
                display_mentions: vec![],
                explicit_action: None,
                envelope: ExecutionEnvelope {
                    effect: Effect::Answer,
                    context: ContextMode::ExplicitReferences,
                    freshness: Freshness::WebRequired,
                    web_reason: WebDecisionReason::LegacyUnknown,
                    verification_requirement: Default::default(),
                    effort: Effort::ToolLoop,
                    security_domain: SecurityDomain::Normal,
                    risk: RiskClass::ReadOnly,
                    modalities: vec![],
                    material_needs: vec![],
                    required_capabilities: vec![],
                    explicit_constraints: vec![],
                    fresh_fact: policy.clone(),
                },
            },
        )
        .expect("accepted run");
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO sqlite_sequence(name, seq) VALUES ('session_evidence', 1000)",
                [],
            )?;
            Ok(())
        })
        .expect("advance evidence ledger sequence");
        let record = FreshDomainRecord::Entertainment(EntertainmentRecord {
            title: "测试影片".into(),
            region: "中国大陆".into(),
            channel: "院线".into(),
            date: "2026-08-25".into(),
            checked_at: "2026-08-25T00:00:00Z".into(),
            origin: EvidenceOrigin {
                evidence_id: 7,
                provider_id: "provider".into(),
                source_url: "https://example.invalid/movie".into(),
                source_title: "影片资料".into(),
                observed_at: "2026-08-25T00:00:00Z".into(),
            },
        });
        let registered = AgentEvidenceRepository::register_external_tool(
            &db,
            ExternalToolEvidenceInput {
                session_id: session.session_id,
                run_id: run_id.clone(),
                message_seq_first: 1,
                title: "entertainment_record".into(),
                provider_id: "provider".into(),
                provider_config_hash: "provider-hash".into(),
                binding_id: "binding".into(),
                raw_result_hash: "result-hash".into(),
                retrieved_at: "2026-08-25T00:00:00Z".into(),
                bounded_excerpt: serde_json::to_string(&record).expect("record JSON"),
                url: Some("https://example.invalid/movie".into()),
                normalized_url: Some("https://example.invalid/movie".into()),
                domain: Some("example.invalid".into()),
            },
        )
        .expect("external evidence");
        assert_eq!(registered.evidence_id, 1001);
        (db, policy, run_id)
    }

    #[test]
    fn host_renderer_uses_dto_fields_and_never_provider_json() {
        let record = FreshDomainRecord::Entertainment(EntertainmentRecord {
            title: "测试影片".into(),
            region: "上海".into(),
            channel: "院线".into(),
            date: "2026-08-20".into(),
            checked_at: "2026-08-19T00:00:00Z".into(),
            origin: EvidenceOrigin {
                evidence_id: 7,
                provider_id: "provider".into(),
                source_url: "https://example.invalid".into(),
                source_title: "title".into(),
                observed_at: "2026-08-19T00:00:00Z".into(),
            },
        });
        let rendered = render_record(&record);
        assert!(rendered.contains("测试影片"));
        assert!(!rendered.contains("provider"));
        assert!(!rendered.contains("source_url"));
    }

    #[test]
    fn host_renderer_never_leaks_provider_metadata_across_all_domain_records() {
        use crate::ai_runtime::fresh_domains::contracts::{
            FinanceRecord, NewsRecord, SportsRecord, WeatherRecord,
        };

        let origin = EvidenceOrigin {
            evidence_id: 1,
            provider_id: "secret-provider".into(),
            source_url: "https://example.invalid/source".into(),
            source_title: "secret-title".into(),
            observed_at: "2026-08-19T00:00:00Z".into(),
        };
        let records = vec![
            FreshDomainRecord::Weather(WeatherRecord {
                location: "北京".into(),
                condition: "晴".into(),
                temperature: "31".into(),
                units: "C".into(),
                observation_time: Some("2026-08-19T00:00:00Z".into()),
                issue_time: None,
                origin: origin.clone(),
            }),
            FreshDomainRecord::News(NewsRecord {
                title: "示例新闻".into(),
                publisher: "示例媒体".into(),
                published_at: "2026-08-19T00:00:00Z".into(),
                topic: Some("科技".into()),
                location: None,
                origin: origin.clone(),
            }),
            FreshDomainRecord::Finance(FinanceRecord {
                instrument: "AAPL".into(),
                asset_kind: "equity".into(),
                currency: "USD".into(),
                as_of: "2026-08-19T00:00:00Z".into(),
                delay: "15 minutes".into(),
                value: "234.56".into(),
                origin: origin.clone(),
            }),
            FreshDomainRecord::Sports(SportsRecord {
                competition: "NBA".into(),
                participants: vec!["Team A".into(), "Team B".into()],
                start_time: "2026-08-19T12:00:00Z".into(),
                status: "live".into(),
                score: Some("1-0".into()),
                checked_at: "2026-08-19T00:00:00Z".into(),
                origin,
            }),
        ];

        for record in records {
            let rendered = render_record(&record);
            assert!(!rendered.contains("secret-provider"));
            assert!(!rendered.contains("secret-title"));
            assert!(!rendered.contains("source_url"));
            assert!(!rendered.contains("example.invalid"));
        }
    }

    #[test]
    fn host_renderer_emits_external_protocol_references_for_high_ledger_ids() {
        let (db, policy, run_id) = accepted_run_with_high_external_evidence_id();

        let submission = render_current_run_submission(&db, &run_id, &policy)
            .expect("render submission")
            .expect("structured evidence submission");

        assert_eq!(submission.blocks[0].sources, ["E1001"]);
        let provenance = AgentEvidenceRepository::provenance_policy(&db, &run_id, false)
            .expect("provenance policy");
        validate_final_answer_submission(&submission, &provenance)
            .expect("host output must use the same protocol as model submissions");
    }
}
