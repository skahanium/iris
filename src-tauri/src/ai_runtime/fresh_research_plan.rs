//! Pure, bounded research planning for frozen current-fact policies.
//!
//! This module turns a `FreshFactPolicy`, the current user message, a locale,
//! and an explicitly confirmed location into one initial Web query plus a
//! frozen research budget. It never reads IP addresses, Vault contents, or
//! provider configuration, and it never includes automatic local material.

use crate::ai_runtime::run_contract::{FreshFactDomain, FreshFactPolicy, LocationRequirement};
use crate::error::{AppError, AppResult};

/// A location the user explicitly confirmed for this Run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfirmedLocation {
    pub(crate) city: Option<String>,
    pub(crate) province: Option<String>,
    pub(crate) country: Option<String>,
}

/// One unresolved evidence gap that may justify a follow-up search.
#[allow(
    dead_code,
    reason = "variants are consumed by Task 4 evidence-gap-driven ToolLoop research"
)]
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
}

/// The initial query and budget frozen before a current-fact Run begins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FreshResearchPlan {
    pub(crate) initial_query: String,
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
    let budget = research_budget(message, policy.domain);
    Ok(FreshResearchPlan {
        initial_query,
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
        "北京", "上海", "广州", "深圳", "杭州", "成都", "重庆", "武汉", "西安", "南京",
        "苏州", "天津", "青岛", "厦门", "香港", "澳门",
    ];
    KNOWN_CITIES
        .iter()
        .find(|city| message.contains(**city))
        .map(|city| (*city).to_string())
}

/// Track already submitted normalized query/gap pairs so the ToolLoop cannot
/// repeat the same research step without consuming a fresh budget slot.
#[derive(Debug, Default)]
pub(crate) struct ResearchQueryLedger {
    seen: Vec<(String, EvidenceGap)>,
}

impl ResearchQueryLedger {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn register(&mut self, query: &str, gap: EvidenceGap) -> AppResult<()> {
        let normalized = normalize_query(query);
        if self
            .seen
            .iter()
            .any(|(seen_query, seen_gap)| seen_query == &normalized && *seen_gap == gap)
        {
            return Err(AppError::msg("fresh_research_duplicate_query"));
        }
        self.seen.push((normalized, gap));
        Ok(())
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

fn research_budget(message: &str, domain: FreshFactDomain) -> ResearchBudget {
    let recommendation = domain == FreshFactDomain::Entertainment
        && contains_any(
            message,
            &["推荐", "有什么好看", "recommend", "suggest", "best"],
        );
    if recommendation {
        ResearchBudget {
            max_searches: 3,
            max_fetches: 5,
            max_repairs: 1,
        }
    } else {
        ResearchBudget {
            max_searches: 2,
            max_fetches: 3,
            max_repairs: 1,
        }
    }
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
                max_searches: 2,
                max_fetches: 3,
                max_repairs: 1,
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
                max_fetches: 5,
                max_repairs: 1,
            }
        );
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
    fn explicit_city_is_detected_only_from_conservative_known_names() {
        assert_eq!(explicit_city_from_message("上海未来一周天气"), Some("上海".into()));
        assert_eq!(explicit_city_from_message("未来一周天气"), None);
        assert_eq!(explicit_city_from_message("我的笔记里有上海"), Some("上海".into()));
    }
}
