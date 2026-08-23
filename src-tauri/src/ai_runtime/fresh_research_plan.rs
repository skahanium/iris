//! Pure, bounded research planning for frozen current-fact policies.
//!
//! This module turns a `FreshFactPolicy`, the current user message, a locale,
//! and an explicitly confirmed location into one initial Web query plus a
//! frozen research budget. It never reads IP addresses, Vault contents, or
//! provider configuration, and it never includes automatic local material.

use crate::ai_runtime::run_contract::{FreshFactDomain, FreshFactPolicy, LocationRequirement};
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};

/// A location the user explicitly confirmed for this Run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfirmedLocation {
    pub(crate) city: Option<String>,
    pub(crate) province: Option<String>,
    pub(crate) country: Option<String>,
}

/// One unresolved evidence gap that may justify a follow-up search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum EvidenceGap {
    MissingEntity,
    MissingLocation,
    LocationCoverage,
    MissingTimestamp,
    StaleObservation,
    MissingUnit,
    MissingChannel,
    MissingIndependentSource,
    SourceConflict,
}

/// Frozen number of research operations for one current-fact Run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResearchBudget {
    pub(crate) max_searches: u8,
    pub(crate) max_fetches: u8,
    pub(crate) max_repairs: u8,
    pub(crate) max_model_continuations: u8,
    pub(crate) max_evidence: u8,
    pub(crate) deadline_seconds: u8,
}

/// User-selected research depth frozen before a current-fact Run begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResearchProfile {
    Quick,
    Standard,
    Deep,
}

impl ResearchProfile {
    fn budget(self) -> ResearchBudget {
        match self {
            Self::Quick => ResearchBudget {
                max_searches: 1,
                max_fetches: 2,
                max_repairs: 1,
                max_model_continuations: 2,
                max_evidence: 4,
                deadline_seconds: 20,
            },
            Self::Standard => ResearchBudget {
                max_searches: 3,
                max_fetches: 6,
                max_repairs: 1,
                max_model_continuations: 4,
                max_evidence: 8,
                deadline_seconds: 45,
            },
            Self::Deep => ResearchBudget {
                max_searches: 5,
                max_fetches: 10,
                max_repairs: 1,
                max_model_continuations: 6,
                max_evidence: 12,
                deadline_seconds: 90,
            },
        }
    }
}

/// The initial query and budget frozen before a current-fact Run begins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FreshResearchPlan {
    pub(crate) initial_query: String,
    pub(crate) profile: ResearchProfile,
    pub(crate) budget: ResearchBudget,
    pub(crate) domain: FreshFactDomain,
}

/// Build the initial research plan for a frozen current-fact policy.
pub(crate) fn build_fresh_research_plan(
    message: &str,
    policy: &FreshFactPolicy,
    _locale: &str,
    location: Option<&ConfirmedLocation>,
) -> AppResult<FreshResearchPlan> {
    if policy.domain == FreshFactDomain::None || policy.domain == FreshFactDomain::Runtime {
        return Err(AppError::msg("agent_run_not_current_fact"));
    }
    if policy.location_requirement == LocationRequirement::City && !confirmed_city(location) {
        return Err(AppError::msg("agent_run_location_required"));
    }

    let initial_query = build_initial_query(message, policy, location);
    let profile = research_profile(message, policy.domain);
    let budget = profile.budget();
    Ok(FreshResearchPlan {
        initial_query,
        profile,
        budget,
        domain: policy.domain,
    })
}

/// Extract a conservative explicit city from the current user message.
///
/// This is only used to decide whether a Run must ask for missing location
/// input. It is not a geocoder and never substitutes for user confirmation.
pub(crate) fn explicit_city_from_message(message: &str) -> Option<String> {
    const KNOWN_CITIES: &[&str] = &[
        "北京", "上海", "广州", "深圳", "杭州", "成都", "重庆", "武汉", "西安", "南京", "苏州",
        "天津", "青岛", "厦门", "香港", "澳门",
    ];
    KNOWN_CITIES
        .iter()
        .find(|city| message.contains(**city))
        .map(|city| (*city).to_string())
}

/// Durable, body-free state for resuming a bounded research plan.
///
/// Query text is intentionally represented only by a hash. The state is a
/// continuation guard, not an evidence or prompt cache.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FreshResearchResumeState {
    pub(crate) schema_version: u8,
    pub(crate) max_searches: u8,
    pub(crate) search_count: u8,
    #[serde(default)]
    pub(crate) max_fetches: u8,
    #[serde(default)]
    pub(crate) fetch_count: u8,
    #[serde(default)]
    pub(crate) max_repairs: u8,
    #[serde(default)]
    pub(crate) repair_count: u8,
    #[serde(default)]
    pub(crate) max_model_continuations: u8,
    #[serde(default)]
    pub(crate) max_evidence: u8,
    #[serde(default)]
    pub(crate) deadline_seconds: u8,
    pub(crate) seen_query_hashes: Vec<String>,
    pub(crate) winner_provider_id: Option<String>,
}

impl FreshResearchResumeState {
    pub(crate) fn validate(&self) -> AppResult<()> {
        if !(self.schema_version == 1 || self.schema_version == 2 || self.schema_version == 3)
            || self.search_count > self.max_searches
            || self.fetch_count > self.max_fetches
            || self.repair_count > self.max_repairs
            || (self.schema_version == 3
                && (self.max_model_continuations == 0
                    || self.max_evidence == 0
                    || self.deadline_seconds == 0))
            || self.seen_query_hashes.len() > usize::from(self.max_searches)
            || self
                .seen_query_hashes
                .iter()
                .any(|hash| hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            return Err(AppError::msg("fresh_research_resume_state_invalid"));
        }
        Ok(())
    }
}

/// Track already submitted normalized queries so the ToolLoop cannot repeat
/// the same research step without consuming a fresh budget slot.
#[derive(Debug, Default)]
pub(crate) struct ResearchQueryLedger {
    seen: Vec<String>,
}

impl ResearchQueryLedger {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn register(&mut self, query: &str, gap: EvidenceGap) -> AppResult<()> {
        let normalized = normalize_query(query);
        let query_hash = crate::cas::hash::content_hash_str(&normalized);
        if self.seen.iter().any(|seen_hash| seen_hash == &query_hash) {
            return Err(AppError::msg("fresh_research_duplicate_query"));
        }
        let _ = gap;
        self.seen.push(query_hash);
        Ok(())
    }

    pub(crate) fn from_hashes(hashes: Vec<String>) -> AppResult<Self> {
        let mut ledger = Self { seen: hashes };
        if ledger
            .seen
            .iter()
            .any(|hash| hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            return Err(AppError::msg("fresh_research_resume_state_invalid"));
        }
        ledger.seen.sort();
        ledger.seen.dedup();
        Ok(ledger)
    }

    pub(crate) fn query_hashes(&self) -> Vec<String> {
        self.seen.clone()
    }
}

fn confirmed_city(location: Option<&ConfirmedLocation>) -> bool {
    location
        .and_then(|location| location.city.as_deref())
        .is_some_and(|city| !city.trim().is_empty())
}

fn build_initial_query(
    message: &str,
    policy: &FreshFactPolicy,
    location: Option<&ConfirmedLocation>,
) -> String {
    let mut query = message.trim().to_string();
    if let Some(city) = location.and_then(|location| location.city.as_deref()) {
        if !city.trim().is_empty() && !query.contains(city.trim()) {
            query.push(' ');
            query.push_str(city.trim());
        }
    }
    if let Some(end) = policy.window_end.as_deref() {
        if let Some(date) = end.split('T').next() {
            query.push(' ');
            query.push_str(date);
        }
    }
    match policy.domain {
        FreshFactDomain::Entertainment => {
            query.push_str(" 院线 流媒体 上映");
        }
        FreshFactDomain::Weather => {
            query.push_str(" 天气预报");
        }
        FreshFactDomain::News => {
            query.push_str(" 新闻");
        }
        FreshFactDomain::Finance => {
            query.push_str(" 股价 行情");
        }
        FreshFactDomain::Sports => {
            query.push_str(" 比赛 赛程");
        }
        FreshFactDomain::GenericWeb => {}
        FreshFactDomain::None | FreshFactDomain::Runtime => {}
    }
    query.chars().take(360).collect()
}

fn research_profile(message: &str, domain: FreshFactDomain) -> ResearchProfile {
    if contains_any(message, &["深入研究", "深度研究", "deep research"]) {
        return ResearchProfile::Deep;
    }
    if domain == FreshFactDomain::Entertainment
        && contains_any(
            message,
            &["推荐", "有什么好看", "recommend", "suggest", "best"],
        )
        || contains_any(
            message,
            &[
                "比较",
                "原因",
                "为什么",
                "综述",
                "前瞻",
                "compare",
                "comparison",
                "why",
                "overview",
                "outlook",
            ],
        )
    {
        return ResearchProfile::Standard;
    }
    ResearchProfile::Quick
}

fn normalize_query(query: &str) -> String {
    query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn contains_any(message: &str, markers: &[&str]) -> bool {
    markers
        .iter()
        .any(|marker| message.to_lowercase().contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::run_contract::FreshFactPolicy;

    fn policy(domain: FreshFactDomain, window_end: Option<&str>) -> FreshFactPolicy {
        FreshFactPolicy {
            schema_version: 1,
            domain,
            operation: None,
            window_start: None,
            window_end: window_end.map(str::to_string),
            location_requirement: match domain {
                FreshFactDomain::Weather | FreshFactDomain::Entertainment => {
                    LocationRequirement::City
                }
                _ => LocationRequirement::None,
            },
        }
    }

    fn shanghai() -> ConfirmedLocation {
        ConfirmedLocation {
            city: Some("上海".to_string()),
            province: Some("上海".to_string()),
            country: Some("中国".to_string()),
        }
    }

    #[test]
    fn absolute_date_and_location_are_included_in_movie_query() {
        let plan = build_fresh_research_plan(
            "最近有什么好看的电影",
            &policy(FreshFactDomain::Entertainment, Some("2026-08-18T08:00:00Z")),
            "zh-CN",
            Some(&shanghai()),
        )
        .expect("plan");

        assert!(plan.initial_query.contains("2026-08-18"));
        assert!(plan.initial_query.contains("上海"));
        assert!(plan.initial_query.contains("院线"));
        assert!(plan.initial_query.contains("流媒体"));
        assert!(!plan.initial_query.contains("历史助手"));
        assert!(!plan.initial_query.contains("本地"));
    }

    #[test]
    fn single_fact_and_recommendation_budgets_are_frozen() {
        let single = build_fresh_research_plan(
            "苹果现在股价多少",
            &policy(FreshFactDomain::Finance, None),
            "zh-CN",
            None,
        )
        .expect("single fact plan");
        assert_eq!(
            single.budget,
            ResearchBudget {
                max_searches: 1,
                max_fetches: 2,
                max_repairs: 1,
                max_model_continuations: 2,
                max_evidence: 4,
                deadline_seconds: 20,
            }
        );

        let recommendation = build_fresh_research_plan(
            "推荐最近有什么好看的电影",
            &policy(FreshFactDomain::Entertainment, Some("2026-08-18T08:00:00Z")),
            "zh-CN",
            Some(&shanghai()),
        )
        .expect("recommendation plan");
        assert_eq!(
            recommendation.budget,
            ResearchBudget {
                max_searches: 3,
                max_fetches: 6,
                max_repairs: 1,
                max_model_continuations: 4,
                max_evidence: 8,
                deadline_seconds: 45,
            }
        );
    }

    #[test]
    fn explicit_deep_request_is_the_only_message_path_to_deep_profile() {
        let deep = build_fresh_research_plan(
            "请深入研究上海本周电影",
            &policy(FreshFactDomain::Entertainment, None),
            "zh-CN",
            Some(&shanghai()),
        )
        .expect("deep plan");
        let standard = build_fresh_research_plan(
            "推荐上海本周电影",
            &policy(FreshFactDomain::Entertainment, None),
            "zh-CN",
            Some(&shanghai()),
        )
        .expect("standard plan");
        let quick = build_fresh_research_plan(
            "苹果现在股价多少",
            &policy(FreshFactDomain::Finance, None),
            "zh-CN",
            None,
        )
        .expect("quick plan");

        assert_eq!(deep.profile, ResearchProfile::Deep);
        assert_eq!(
            deep.budget,
            ResearchBudget {
                max_searches: 5,
                max_fetches: 10,
                max_repairs: 1,
                max_model_continuations: 6,
                max_evidence: 12,
                deadline_seconds: 90,
            }
        );
        assert_eq!(standard.profile, ResearchProfile::Standard);
        assert_eq!(quick.profile, ResearchProfile::Quick);
    }

    #[test]
    fn city_requirement_without_confirmed_location_fails_closed() {
        let error = build_fresh_research_plan(
            "上海未来一周天气",
            &policy(FreshFactDomain::Weather, None),
            "zh-CN",
            None,
        )
        .expect_err("city required");
        assert_eq!(error.to_string(), "agent_run_location_required");
    }

    #[test]
    fn duplicate_normalized_query_and_gap_are_rejected() {
        let mut ledger = ResearchQueryLedger::new();
        ledger
            .register("  上海 电影 2026-08-18 ", EvidenceGap::MissingEntity)
            .expect("first register");
        let error = ledger
            .register("上海 电影 2026-08-18", EvidenceGap::MissingEntity)
            .expect_err("duplicate");
        assert_eq!(error.to_string(), "fresh_research_duplicate_query");
    }

    #[test]
    fn duplicate_normalized_query_is_rejected_even_when_gap_changes() {
        let mut ledger = ResearchQueryLedger::new();
        ledger
            .register("上海 电影", EvidenceGap::MissingEntity)
            .expect("first register");
        let error = ledger
            .register(" 上海   电影 ", EvidenceGap::MissingTimestamp)
            .expect_err("same query cannot be retried under another gap");
        assert_eq!(error.to_string(), "fresh_research_duplicate_query");
    }

    #[test]
    fn resume_state_rejects_fetch_usage_above_the_frozen_limit() {
        let state = FreshResearchResumeState {
            schema_version: 2,
            max_searches: 1,
            search_count: 1,
            max_fetches: 2,
            fetch_count: 3,
            max_repairs: 1,
            repair_count: 0,
            max_model_continuations: 2,
            max_evidence: 4,
            deadline_seconds: 20,
            seen_query_hashes: Vec::new(),
            winner_provider_id: None,
        };

        assert_eq!(
            state
                .validate()
                .expect_err("over-limit fetches fail")
                .to_string(),
            "fresh_research_resume_state_invalid"
        );
    }

    #[test]
    fn legacy_resume_state_defaults_new_budget_counters() {
        let state: FreshResearchResumeState = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "maxSearches": 2,
            "searchCount": 1,
            "seenQueryHashes": [],
            "winnerProviderId": null
        }))
        .expect("legacy state deserializes");

        state.validate().expect("legacy state remains resumable");
        assert_eq!(state.fetch_count, 0);
        assert_eq!(state.repair_count, 0);
    }

    #[test]
    fn explicit_city_is_detected_only_from_conservative_known_names() {
        assert_eq!(
            explicit_city_from_message("上海未来一周天气"),
            Some("上海".into())
        );
        assert_eq!(explicit_city_from_message("未来一周天气"), None);
        assert_eq!(
            explicit_city_from_message("我的笔记里有上海"),
            Some("上海".into())
        );
    }
}
