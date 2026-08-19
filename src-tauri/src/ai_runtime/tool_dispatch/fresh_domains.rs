//! Parameter validation for stable Iris current-fact domain tools.
//!
//! Task 4 owns provider resolution and the real `FreshDomainService`. Until
//! that service exists this dispatcher validates the normalized request and
//! returns a stable "service not ready" error instead of fabricating provider
//! data.

use chrono::{DateTime, NaiveDate, Utc};

use crate::ai_runtime::fresh_domains::location::resolve_confirmed_location;
use crate::ai_runtime::fresh_domains::service::{FreshDomainRequest, FreshDomainService};
use crate::ai_runtime::mcp_external_tools::DomainOperation;
use crate::ai_runtime::tool_dispatch::read_global_memories;
use crate::error::{AppError, AppResult};

use super::{FrozenDomainWindow, ToolDispatchContext};

const MAX_WEATHER_DAYS: i64 = 7;
const MAX_NEWS_LIMIT: i64 = 20;

const ERROR_UNKNOWN_OPERATION: &str = "agent_run_fresh_domain_unknown_operation";
const ERROR_BUDGET_EXCEEDED: &str = "agent_run_fresh_domain_budget_exceeded";
const ERROR_MISSING_INSTRUMENT: &str = "agent_run_fresh_domain_missing_instrument";
const ERROR_LOCATION_REQUIRED: &str = "agent_run_location_required";
const ERROR_INVALID_DATE: &str = "agent_run_fresh_domain_invalid_date";
const ERROR_DATE_OUTSIDE_WINDOW: &str = "agent_run_fresh_domain_date_outside_frozen_window";
const ERROR_FROZEN_WINDOW_MISSING: &str = "agent_run_fresh_domain_frozen_window_missing";

pub(super) async fn fresh_domain_tool(
    _state: &crate::app::AppState,
    ctx: &ToolDispatchContext<'_>,
    tool_name: &str,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    if !ctx.web_search_enabled {
        return Err(AppError::msg("web search not enabled for this request"));
    }
    let normalized_args = fill_confirmed_city_into_args(tool_name, args, ctx)?;
    validate_fresh_domain_request(tool_name, &normalized_args, ctx.fresh_fact_policy.as_ref())?;
    let operation = parse_operation_for_tool(tool_name, &normalized_args)?;
    let records = FreshDomainService
        .execute(
            FreshDomainRequest {
                tool_name: tool_name.to_string(),
                operation,
                args: normalized_args,
                requested_at: Utc::now(),
                location_gap: None,
            },
            ctx,
        )
        .await?;
    Ok(serde_json::to_value(records)?)
}

/// Fill a missing city for city-required tools from confirmed global memory.
///
/// The pure validator stays strict; this enrichment only happens in the real
/// dispatch path where global memories are available.
fn fill_confirmed_city_into_args(
    tool_name: &str,
    args: &serde_json::Value,
    ctx: &ToolDispatchContext<'_>,
) -> AppResult<serde_json::Value> {
    if !matches!(tool_name, "weather_lookup" | "entertainment_lookup") {
        return Ok(args.clone());
    }
    let has_location = args
        .get("location")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    if has_location {
        return Ok(args.clone());
    }
    let Some(db) = ctx.db else {
        return Ok(args.clone());
    };
    let memories = read_global_memories(db)?;
    let confirmed = resolve_confirmed_location(None, &memories);
    let Some(city) = confirmed
        .city
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(args.clone());
    };
    let mut normalized = args.clone();
    if let Some(object) = normalized.as_object_mut() {
        object.insert(
            "location".into(),
            serde_json::Value::String(city.to_string()),
        );
    }
    Ok(normalized)
}

pub(super) fn validate_fresh_domain_request(
    tool_name: &str,
    args: &serde_json::Value,
    policy: Option<&FrozenDomainWindow>,
) -> AppResult<()> {
    match tool_name {
        "weather_lookup" => validate_weather(args, policy)?,
        "news_lookup" => validate_news(args, policy)?,
        "finance_lookup" => validate_finance(args)?,
        "entertainment_lookup" => validate_entertainment(args)?,
        "sports_lookup" => validate_sports(args, policy)?,
        _ => return Err(AppError::msg(format!("unknown tool: {tool_name}"))),
    }
    Ok(())
}

fn validate_weather(
    args: &serde_json::Value,
    _policy: Option<&FrozenDomainWindow>,
) -> AppResult<()> {
    parse_operation_for(
        args,
        &[
            DomainOperation::WeatherCurrent,
            DomainOperation::WeatherForecast,
        ],
    )?;
    require_location(args, "location")?;
    if let Some(days) = optional_integer(args, "days", MAX_WEATHER_DAYS)? {
        if !(1..=MAX_WEATHER_DAYS).contains(&days) {
            return Err(AppError::msg(ERROR_BUDGET_EXCEEDED));
        }
    }
    Ok(())
}

fn validate_news(args: &serde_json::Value, policy: Option<&FrozenDomainWindow>) -> AppResult<()> {
    if let Some(limit) = optional_integer(args, "limit", MAX_NEWS_LIMIT)? {
        if !(1..=MAX_NEWS_LIMIT).contains(&limit) {
            return Err(AppError::msg(ERROR_BUDGET_EXCEEDED));
        }
    }
    if let Some(start) = optional_string(args, "start")? {
        validate_date_in_frozen_window(start, policy)?;
    }
    if let Some(end) = optional_string(args, "end")? {
        validate_date_in_frozen_window(end, policy)?;
    }
    Ok(())
}

fn validate_finance(args: &serde_json::Value) -> AppResult<()> {
    parse_operation_for(
        args,
        &[
            DomainOperation::FinanceQuote,
            DomainOperation::FinanceMetrics,
            DomainOperation::FinanceNews,
        ],
    )?;
    require_non_empty(args, "instrument", ERROR_MISSING_INSTRUMENT)?;
    Ok(())
}

fn validate_entertainment(args: &serde_json::Value) -> AppResult<()> {
    let operation = parse_operation_for(
        args,
        &[
            DomainOperation::EntertainmentNowPlaying,
            DomainOperation::EntertainmentUpcoming,
            DomainOperation::EntertainmentStreaming,
        ],
    )?;
    if operation == DomainOperation::EntertainmentNowPlaying {
        require_location(args, "location")?;
    }
    Ok(())
}

fn validate_sports(args: &serde_json::Value, policy: Option<&FrozenDomainWindow>) -> AppResult<()> {
    parse_operation_for(
        args,
        &[
            DomainOperation::SportsSchedule,
            DomainOperation::SportsScore,
        ],
    )?;
    if let Some(date) = optional_string(args, "date")? {
        validate_date_in_frozen_window(date, policy)?;
    }
    Ok(())
}

fn parse_operation_for_tool(
    tool_name: &str,
    args: &serde_json::Value,
) -> AppResult<DomainOperation> {
    match tool_name {
        "weather_lookup" => parse_operation_for(
            args,
            &[
                DomainOperation::WeatherCurrent,
                DomainOperation::WeatherForecast,
            ],
        ),
        "news_lookup" => Ok(DomainOperation::NewsSearch),
        "finance_lookup" => parse_operation_for(
            args,
            &[
                DomainOperation::FinanceQuote,
                DomainOperation::FinanceMetrics,
                DomainOperation::FinanceNews,
            ],
        ),
        "entertainment_lookup" => parse_operation_for(
            args,
            &[
                DomainOperation::EntertainmentNowPlaying,
                DomainOperation::EntertainmentUpcoming,
                DomainOperation::EntertainmentStreaming,
            ],
        ),
        "sports_lookup" => parse_operation_for(
            args,
            &[
                DomainOperation::SportsSchedule,
                DomainOperation::SportsScore,
            ],
        ),
        _ => Err(AppError::msg(ERROR_UNKNOWN_OPERATION)),
    }
}

fn parse_operation_for(
    args: &serde_json::Value,
    allowed: &[DomainOperation],
) -> AppResult<DomainOperation> {
    let raw = args
        .get("operation")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| AppError::msg(ERROR_UNKNOWN_OPERATION))?;
    let operation =
        DomainOperation::parse(raw).ok_or_else(|| AppError::msg(ERROR_UNKNOWN_OPERATION))?;
    if allowed.contains(&operation) {
        Ok(operation)
    } else {
        Err(AppError::msg(ERROR_UNKNOWN_OPERATION))
    }
}

fn require_location(args: &serde_json::Value, key: &str) -> AppResult<()> {
    require_non_empty(args, key, ERROR_LOCATION_REQUIRED)
}

fn require_non_empty(args: &serde_json::Value, key: &str, code: &str) -> AppResult<()> {
    let value = args
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| AppError::msg(code))?;
    if value.trim().is_empty() {
        Err(AppError::msg(code))
    } else {
        Ok(())
    }
}

fn optional_string<'a>(args: &'a serde_json::Value, key: &str) -> AppResult<Option<&'a str>> {
    match args.get(key) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(Some)
            .ok_or_else(|| AppError::msg(format!("{key} must be a string"))),
    }
}

fn optional_integer(args: &serde_json::Value, key: &str, max: i64) -> AppResult<Option<i64>> {
    match args.get(key) {
        None => Ok(None),
        Some(value) => {
            let number = value
                .as_i64()
                .ok_or_else(|| AppError::msg(ERROR_BUDGET_EXCEEDED))?;
            if number < 1 || number > max {
                Err(AppError::msg(ERROR_BUDGET_EXCEEDED))
            } else {
                Ok(Some(number))
            }
        }
    }
}

fn validate_date_in_frozen_window(
    value: &str,
    policy: Option<&FrozenDomainWindow>,
) -> AppResult<()> {
    let date = parse_date_arg(value)?;
    let policy = policy.ok_or_else(|| AppError::msg(ERROR_FROZEN_WINDOW_MISSING))?;
    if let Some(start) = policy.window_start.as_deref() {
        let start = parse_rfc3339(start)?;
        if date < start {
            return Err(AppError::msg(ERROR_DATE_OUTSIDE_WINDOW));
        }
    }
    if let Some(end) = policy.window_end.as_deref() {
        let end = parse_rfc3339(end)?;
        if date > end {
            return Err(AppError::msg(ERROR_DATE_OUTSIDE_WINDOW));
        }
    }
    Ok(())
}

fn parse_date_arg(value: &str) -> AppResult<DateTime<Utc>> {
    if let Ok(date_time) = DateTime::parse_from_rfc3339(value) {
        return Ok(date_time.with_timezone(&Utc));
    }
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| AppError::msg(ERROR_INVALID_DATE))?;
    date.and_hms_opt(0, 0, 0)
        .map(|date_time| date_time.and_utc())
        .ok_or_else(|| AppError::msg(ERROR_INVALID_DATE))
}

fn parse_rfc3339(value: &str) -> AppResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|date_time| date_time.with_timezone(&Utc))
        .map_err(|_| AppError::msg(ERROR_INVALID_DATE))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(start: Option<&str>, end: Option<&str>) -> FrozenDomainWindow {
        FrozenDomainWindow {
            window_start: start.map(str::to_string),
            window_end: end.map(str::to_string),
        }
    }

    #[test]
    fn fresh_domain_tool_rejects_unknown_operation() {
        let error = validate_fresh_domain_request(
            "weather_lookup",
            &serde_json::json!({ "operation": "weather.nope", "location": "北京" }),
            None,
        )
        .expect_err("unknown operation must be rejected");
        assert_eq!(error.to_string(), ERROR_UNKNOWN_OPERATION);
    }

    #[test]
    fn fresh_domain_tool_rejects_oversized_weather_days() {
        let error = validate_fresh_domain_request(
            "weather_lookup",
            &serde_json::json!({ "operation": "weather.forecast", "location": "北京", "days": 8 }),
            None,
        )
        .expect_err("days over budget must be rejected");
        assert_eq!(error.to_string(), ERROR_BUDGET_EXCEEDED);
    }

    #[test]
    fn fresh_domain_tool_rejects_oversized_news_limit() {
        let error = validate_fresh_domain_request(
            "news_lookup",
            &serde_json::json!({ "topic": "科技", "limit": 21 }),
            None,
        )
        .expect_err("limit over budget must be rejected");
        assert_eq!(error.to_string(), ERROR_BUDGET_EXCEEDED);
    }

    #[test]
    fn fresh_domain_tool_rejects_finance_without_instrument() {
        let error = validate_fresh_domain_request(
            "finance_lookup",
            &serde_json::json!({ "operation": "finance.quote" }),
            None,
        )
        .expect_err("finance without instrument must be rejected");
        assert_eq!(error.to_string(), ERROR_MISSING_INSTRUMENT);
    }

    #[test]
    fn fresh_domain_tool_rejects_weather_without_city() {
        let error = validate_fresh_domain_request(
            "weather_lookup",
            &serde_json::json!({ "operation": "weather.current" }),
            None,
        )
        .expect_err("weather without city must be rejected");
        assert_eq!(error.to_string(), ERROR_LOCATION_REQUIRED);
    }

    #[test]
    fn fresh_domain_tool_rejects_now_playing_without_city() {
        let error = validate_fresh_domain_request(
            "entertainment_lookup",
            &serde_json::json!({ "operation": "entertainment.now_playing" }),
            None,
        )
        .expect_err("now_playing without city must be rejected");
        assert_eq!(error.to_string(), ERROR_LOCATION_REQUIRED);
    }

    #[test]
    fn fresh_domain_tool_rejects_date_outside_frozen_window() {
        let p = policy(Some("2026-08-18T00:00:00Z"), Some("2026-08-25T00:00:00Z"));
        let error = validate_fresh_domain_request(
            "sports_lookup",
            &serde_json::json!({ "operation": "sports.schedule", "date": "2026-09-01" }),
            Some(&p),
        )
        .expect_err("date outside frozen window must be rejected");
        assert_eq!(error.to_string(), ERROR_DATE_OUTSIDE_WINDOW);
    }

    #[test]
    fn fresh_domain_tool_accepts_valid_request_validation() {
        let p = policy(Some("2026-08-18T00:00:00Z"), Some("2026-08-25T00:00:00Z"));
        validate_fresh_domain_request(
            "sports_lookup",
            &serde_json::json!({ "operation": "sports.score", "date": "2026-08-20" }),
            Some(&p),
        )
        .expect("valid request must pass validation");
        parse_operation_for_tool(
            "sports_lookup",
            &serde_json::json!({ "operation": "sports.score" }),
        )
        .expect("operation parser returns the validated operation");
    }
}
