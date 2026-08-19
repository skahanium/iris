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
            sources: vec![evidence_id.to_string()],
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
    use super::render_record;
    use crate::ai_runtime::fresh_domains::contracts::{
        EntertainmentRecord, EvidenceOrigin, FreshDomainRecord,
    };

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
}
