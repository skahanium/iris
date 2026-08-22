//! Deterministic, model-free classification of fresh-fact requests.
//!
//! The classifier freezes a backward-compatible `FreshFactPolicy` into the
//! accepted `ExecutionEnvelope`. It never calls a model, reads the clock beyond
//! the supplied `accepted_at`, or consults provider/Vault configuration.

use chrono::{DateTime, Duration, Utc};

use crate::ai_runtime::run_contract::{
    DomainOperation, FreshFactDomain, FreshFactPolicy, LocationRequirement,
};

/// Classify a user message into a frozen current-fact policy.
///
/// The returned policy is intentionally pure: all time windows are derived from
/// `accepted_at` so tests and recovery can reproduce the same decision.
pub(crate) fn classify_fresh_fact(message: &str, accepted_at: DateTime<Utc>) -> FreshFactPolicy {
    let lower = message.to_lowercase();
    let domain = classify_domain(&lower);
    let operation = classify_domain_operation(&lower, domain);
    let (window_start, window_end, location_requirement) = match domain {
        FreshFactDomain::News => (
            Some(accepted_at - Duration::hours(72)),
            Some(accepted_at),
            LocationRequirement::None,
        ),
        FreshFactDomain::Entertainment => (
            Some(accepted_at - Duration::days(30)),
            Some(accepted_at + Duration::days(60)),
            if operation == Some(DomainOperation::EntertainmentNowPlaying) {
                LocationRequirement::City
            } else {
                LocationRequirement::None
            },
        ),
        FreshFactDomain::Sports => (
            Some(accepted_at),
            Some(accepted_at + Duration::days(7)),
            LocationRequirement::None,
        ),
        FreshFactDomain::Weather => (
            Some(accepted_at),
            Some(accepted_at + Duration::days(7)),
            LocationRequirement::City,
        ),
        FreshFactDomain::None
        | FreshFactDomain::Runtime
        | FreshFactDomain::Finance
        | FreshFactDomain::GenericWeb => (None, None, LocationRequirement::None),
    };

    FreshFactPolicy {
        schema_version: 1,
        domain,
        operation,
        window_start: window_start.map(|value| value.to_rfc3339()),
        window_end: window_end.map(|value| value.to_rfc3339()),
        location_requirement,
    }
}

fn classify_domain(message: &str) -> FreshFactDomain {
    if is_runtime_request(message) {
        return FreshFactDomain::Runtime;
    }
    if contains_any(
        message,
        &[
            "天气",
            "weather",
            "气温",
            "温度",
            "降雨",
            "降水",
            "forecast",
            "temperature",
        ],
    ) {
        return FreshFactDomain::Weather;
    }
    if contains_any(
        message,
        &[
            "股价",
            "股票",
            "行情",
            "股市",
            "金融",
            "汇率",
            "stock price",
            "stock",
            "finance",
            "market",
            "exchange rate",
            "quote",
        ],
    ) {
        return FreshFactDomain::Finance;
    }
    if contains_any(
        message,
        &[
            "新闻",
            "头条",
            "要闻",
            "时事",
            "news",
            "headline",
            "headlines",
            "breaking",
        ],
    ) {
        return FreshFactDomain::News;
    }
    if is_current_entertainment_request(message) {
        return FreshFactDomain::Entertainment;
    }
    if contains_any(
        message,
        &[
            "比赛", "赛况", "战况", "比分", "赛程", "球队", "湖人", "game", "score", "match",
            "fixture", "sports", "nba", "英超", "西甲", "中超",
        ],
    ) {
        return FreshFactDomain::Sports;
    }
    if contains_any(
        message,
        &[
            "最新",
            "当前",
            "现在",
            "实时",
            "today",
            "current",
            "latest",
            "now",
            "最新情况",
            "当前动态",
            "实时动态",
            "current events",
            "today's news",
            "what is happening now",
        ],
    ) {
        return FreshFactDomain::GenericWeb;
    }
    FreshFactDomain::None
}

/// Select the one operation that an accepted current-fact Run may execute.
pub(crate) fn classify_domain_operation(
    message: &str,
    domain: FreshFactDomain,
) -> Option<DomainOperation> {
    match domain {
        FreshFactDomain::Weather => Some(
            if contains_any(
                message,
                &[
                    "预报",
                    "未来",
                    "明天",
                    "后天",
                    "forecast",
                    "tomorrow",
                    "next week",
                ],
            ) {
                DomainOperation::WeatherForecast
            } else {
                DomainOperation::WeatherCurrent
            },
        ),
        FreshFactDomain::News => Some(DomainOperation::NewsSearch),
        FreshFactDomain::Finance => Some(
            if contains_any(message, &["新闻", "news", "要闻", "headline", "headlines"]) {
                DomainOperation::FinanceNews
            } else if contains_any(
                message,
                &[
                    "指标",
                    "metrics",
                    "市盈率",
                    "pe ratio",
                    "市值",
                    "market cap",
                ],
            ) {
                DomainOperation::FinanceMetrics
            } else {
                DomainOperation::FinanceQuote
            },
        ),
        FreshFactDomain::Entertainment => Some(
            if contains_any(message, &["流媒体", "现在能看", "streaming", "stream"]) {
                DomainOperation::EntertainmentStreaming
            } else if contains_any(message, &["即将上映", "将上映", "upcoming", "coming soon"])
            {
                DomainOperation::EntertainmentUpcoming
            } else {
                DomainOperation::EntertainmentNowPlaying
            },
        ),
        FreshFactDomain::Sports => Some(
            if contains_any(
                message,
                &["赛程", "下一场", "下场", "schedule", "fixture", "next game"],
            ) {
                DomainOperation::SportsSchedule
            } else {
                DomainOperation::SportsScore
            },
        ),
        FreshFactDomain::None | FreshFactDomain::Runtime | FreshFactDomain::GenericWeb => None,
    }
}

fn is_runtime_request(message: &str) -> bool {
    contains_any(
        message,
        &[
            "今天是几月几日",
            "今天几月几日",
            "今天是几号",
            "今天几号",
            "当前日期",
            "本机日期",
            "现在几点",
            "当前时间",
            "本机时间",
            "应用版本",
            "iris 版本",
            "今天星期几",
            "what day of the week is it today",
            "which day of the week is it today",
            "what day is it",
            "what day of week is it",
            "what is today's weekday",
            "what is today's date",
            "current local time",
            "what is the local time",
            "what time is it locally",
            "show local date",
            "app version",
            "application version",
            "iris version",
        ],
    )
}

fn is_current_entertainment_request(message: &str) -> bool {
    let has_entertainment = contains_any(
        message,
        &[
            "电影",
            "影片",
            "movie",
            "movies",
            "film",
            "上映",
            "院线",
            "流媒体",
            "现在能看",
            "theater",
            "theatre",
            "streaming",
            "now showing",
        ],
    );
    if !has_entertainment {
        return false;
    }
    let has_current_window = contains_any(
        message,
        &[
            "最近",
            "近期",
            "最新",
            "今天",
            "今晚",
            "上映",
            "院线",
            "流媒体",
            "现在能看",
            "current",
            "latest",
            "now",
            "release",
            "theater",
            "theatre",
            "streaming",
        ],
    );
    // A pure history/review request is not a current-fact domain.
    let asks_history = contains_any(message, &["历史", "影评", "老电影", "review", "classic"]);
    has_current_window && !asks_history
}

fn contains_any(message: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| message.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::mcp_external_tools::DomainOperation;
    use crate::ai_runtime::run_contract::FreshFactDomain;

    fn accepted_at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-18T08:00:00Z")
            .expect("fixed test time")
            .with_timezone(&Utc)
    }

    #[test]
    fn classifies_fresh_fact_domains_table_driven() {
        let cases = [
            ("今天是几月几日", FreshFactDomain::Runtime),
            ("最近有什么好看的电影", FreshFactDomain::Entertainment),
            ("上海未来一周天气", FreshFactDomain::Weather),
            ("今天有什么重要新闻", FreshFactDomain::News),
            ("苹果现在股价多少", FreshFactDomain::Finance),
            ("今晚湖人比赛几点", FreshFactDomain::Sports),
            ("解释量子计算", FreshFactDomain::None),
        ];

        for (message, expected) in cases {
            let policy = classify_fresh_fact(message, accepted_at());
            assert_eq!(policy.domain, expected, "unexpected domain for {message:?}");
        }
    }

    #[test]
    fn freezes_one_operation_and_only_requires_city_when_needed() {
        let cases = [
            (
                "上海未来一周天气",
                Some(DomainOperation::WeatherForecast),
                LocationRequirement::City,
            ),
            (
                "上海现在天气",
                Some(DomainOperation::WeatherCurrent),
                LocationRequirement::City,
            ),
            (
                "苹果股票最新新闻",
                Some(DomainOperation::FinanceNews),
                LocationRequirement::None,
            ),
            (
                "美元兑人民币汇率指标",
                Some(DomainOperation::FinanceMetrics),
                LocationRequirement::None,
            ),
            (
                "苹果现在股价",
                Some(DomainOperation::FinanceQuote),
                LocationRequirement::None,
            ),
            (
                "本周有什么新片即将上映",
                Some(DomainOperation::EntertainmentUpcoming),
                LocationRequirement::None,
            ),
            (
                "现在能看什么流媒体电影",
                Some(DomainOperation::EntertainmentStreaming),
                LocationRequirement::None,
            ),
            (
                "上海正在上映什么电影",
                Some(DomainOperation::EntertainmentNowPlaying),
                LocationRequirement::City,
            ),
            (
                "湖人下一场比赛",
                Some(DomainOperation::SportsSchedule),
                LocationRequirement::None,
            ),
            (
                "湖人比分",
                Some(DomainOperation::SportsScore),
                LocationRequirement::None,
            ),
            (
                "今天有什么重要新闻",
                Some(DomainOperation::NewsSearch),
                LocationRequirement::None,
            ),
            ("今天是几月几日", None, LocationRequirement::None),
        ];

        for (message, expected_operation, expected_location) in cases {
            let policy = classify_fresh_fact(message, accepted_at());
            assert_eq!(
                policy.operation, expected_operation,
                "unexpected operation for {message:?}"
            );
            assert_eq!(
                policy.location_requirement, expected_location,
                "unexpected location for {message:?}"
            );
        }
    }

    #[test]
    fn fresh_fact_windows_are_frozen_relative_to_accepted_at() {
        let cases = [
            (
                "今天有什么重要新闻",
                FreshFactDomain::News,
                Some(accepted_at() - Duration::hours(72)),
                Some(accepted_at()),
            ),
            (
                "最近有什么好看的电影",
                FreshFactDomain::Entertainment,
                Some(accepted_at() - Duration::days(30)),
                Some(accepted_at() + Duration::days(60)),
            ),
            (
                "今晚湖人比赛几点",
                FreshFactDomain::Sports,
                Some(accepted_at()),
                Some(accepted_at() + Duration::days(7)),
            ),
            (
                "上海未来一周天气",
                FreshFactDomain::Weather,
                Some(accepted_at()),
                Some(accepted_at() + Duration::days(7)),
            ),
        ];

        for (message, domain, expected_start, expected_end) in cases {
            let policy = classify_fresh_fact(message, accepted_at());
            assert_eq!(policy.domain, domain);
            assert_eq!(
                policy.window_start,
                expected_start.map(|value| value.to_rfc3339())
            );
            assert_eq!(
                policy.window_end,
                expected_end.map(|value| value.to_rfc3339())
            );
        }
    }

    #[test]
    fn pure_history_movie_review_is_not_current_entertainment() {
        let policy = classify_fresh_fact("推荐一部经典老电影的历史影评", accepted_at());
        assert_eq!(policy.domain, FreshFactDomain::None);
    }
}
