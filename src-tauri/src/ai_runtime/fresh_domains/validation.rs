//! Deterministic validation for normalized current-fact domain records.
//!
//! The validator is pure: it never calls a model, accesses the network, or
//! writes a database. Failures use stable, safe error-code strings so callers
//! can classify them without parsing provider prose.

use chrono::{DateTime, Duration, NaiveDate, Utc};

use super::contracts::{
    DomainOperation, EntertainmentRecord, EvidenceOrigin, FinanceRecord, FreshDomainRecord,
    NewsRecord, SportsRecord, WeatherRecord, MAX_ASSET_KIND_CHARS, MAX_CHANNEL_CHARS,
    MAX_COMPETITION_CHARS, MAX_CONDITION_CHARS, MAX_CURRENCY_CHARS, MAX_DATE_CHARS,
    MAX_DELAY_CHARS, MAX_INSTRUMENT_CHARS, MAX_LOCATION_CHARS, MAX_PARTICIPANTS,
    MAX_PARTICIPANT_CHARS, MAX_PROVIDER_ID_CHARS, MAX_PUBLISHER_CHARS, MAX_REGION_CHARS,
    MAX_SCORE_CHARS, MAX_SOURCE_TITLE_CHARS, MAX_SOURCE_URL_CHARS, MAX_STATUS_CHARS,
    MAX_TEMPERATURE_CHARS, MAX_TIME_CHARS, MAX_TITLE_CHARS, MAX_TOPIC_CHARS, MAX_UNITS_CHARS,
    MAX_VALUE_CHARS,
};

const ERROR_DOMAIN_INVALID: &str = "agent_run_fresh_domain_invalid";
const ERROR_EVIDENCE_INSUFFICIENT: &str = "agent_run_fresh_evidence_insufficient";
const ERROR_HTTPS_REQUIRED: &str = "agent_run_fresh_https_required";
const ERROR_INVALID_TIME: &str = "agent_run_fresh_invalid_time";
const ERROR_STALE: &str = "agent_run_fresh_evidence_stale";
const ERROR_UNKNOWN_UNIT: &str = "agent_run_fresh_unknown_unit";
const ERROR_FIELD_TOO_LONG: &str = "agent_run_fresh_field_too_long";
const ERROR_LOCATION_REQUIRED: &str = "agent_run_location_required";

/// Validate one domain record against its operation, Appendix D freshness
/// thresholds, required fields, and source HTTPS policy.
pub(crate) fn validate_domain_record(
    operation: DomainOperation,
    requested_at: DateTime<Utc>,
    record: &FreshDomainRecord,
) -> crate::error::AppResult<()> {
    validate_operation_variant(operation, record)?;
    match (operation, record) {
        (DomainOperation::WeatherCurrent, FreshDomainRecord::Weather(record)) => {
            validate_weather_current(record, requested_at)
        }
        (DomainOperation::WeatherForecast, FreshDomainRecord::Weather(record)) => {
            validate_weather_forecast(record, requested_at)
        }
        (DomainOperation::NewsSearch, FreshDomainRecord::News(record)) => validate_news(record),
        (
            DomainOperation::FinanceQuote
            | DomainOperation::FinanceMetrics
            | DomainOperation::FinanceNews,
            FreshDomainRecord::Finance(record),
        ) => validate_finance(record),
        (
            DomainOperation::EntertainmentNowPlaying
            | DomainOperation::EntertainmentUpcoming
            | DomainOperation::EntertainmentStreaming,
            FreshDomainRecord::Entertainment(record),
        ) => validate_entertainment(record, requested_at),
        (DomainOperation::SportsSchedule, FreshDomainRecord::Sports(record)) => {
            validate_sports_schedule(record, requested_at)
        }
        (DomainOperation::SportsScore, FreshDomainRecord::Sports(record)) => {
            validate_sports_score(record, requested_at)
        }
        _ => unreachable!("operation/record variant match was checked above"),
    }
}

fn validate_operation_variant(
    operation: DomainOperation,
    record: &FreshDomainRecord,
) -> crate::error::AppResult<()> {
    let matches = matches!(
        (operation, record),
        (
            DomainOperation::WeatherCurrent | DomainOperation::WeatherForecast,
            FreshDomainRecord::Weather(_),
        ) | (DomainOperation::NewsSearch, FreshDomainRecord::News(_))
            | (
                DomainOperation::FinanceQuote
                    | DomainOperation::FinanceMetrics
                    | DomainOperation::FinanceNews,
                FreshDomainRecord::Finance(_),
            )
            | (
                DomainOperation::EntertainmentNowPlaying
                    | DomainOperation::EntertainmentUpcoming
                    | DomainOperation::EntertainmentStreaming,
                FreshDomainRecord::Entertainment(_),
            )
            | (
                DomainOperation::SportsSchedule | DomainOperation::SportsScore,
                FreshDomainRecord::Sports(_),
            )
    );
    if matches {
        Ok(())
    } else {
        Err(crate::error::AppError::msg(ERROR_DOMAIN_INVALID))
    }
}

fn validate_weather_current(
    record: &WeatherRecord,
    requested_at: DateTime<Utc>,
) -> crate::error::AppResult<()> {
    validate_origin(&record.origin)?;
    validate_required_location(&record.location)?;
    validate_budget(&record.condition, MAX_CONDITION_CHARS)?;
    validate_budget(&record.temperature, MAX_TEMPERATURE_CHARS)?;
    validate_budget(&record.units, MAX_UNITS_CHARS)?;
    validate_weather_units(&record.units)?;
    let observation_time = record
        .observation_time
        .as_deref()
        .ok_or_else(|| crate::error::AppError::msg(ERROR_EVIDENCE_INSUFFICIENT))?;
    validate_not_stale(
        observation_time,
        requested_at,
        Duration::hours(3),
        ERROR_STALE,
    )
}

fn validate_weather_forecast(
    record: &WeatherRecord,
    requested_at: DateTime<Utc>,
) -> crate::error::AppResult<()> {
    validate_origin(&record.origin)?;
    validate_required_location(&record.location)?;
    validate_budget(&record.condition, MAX_CONDITION_CHARS)?;
    validate_budget(&record.temperature, MAX_TEMPERATURE_CHARS)?;
    validate_budget(&record.units, MAX_UNITS_CHARS)?;
    validate_weather_units(&record.units)?;
    let issue_time = record
        .issue_time
        .as_deref()
        .ok_or_else(|| crate::error::AppError::msg(ERROR_EVIDENCE_INSUFFICIENT))?;
    validate_not_stale(issue_time, requested_at, Duration::hours(12), ERROR_STALE)
}

fn validate_news(record: &NewsRecord) -> crate::error::AppResult<()> {
    validate_origin(&record.origin)?;
    validate_required(&record.title, ERROR_EVIDENCE_INSUFFICIENT)?;
    validate_required(&record.publisher, ERROR_EVIDENCE_INSUFFICIENT)?;
    validate_required(&record.published_at, ERROR_EVIDENCE_INSUFFICIENT)?;
    validate_budget(&record.title, MAX_TITLE_CHARS)?;
    validate_budget(&record.publisher, MAX_PUBLISHER_CHARS)?;
    if let Some(topic) = record.topic.as_deref() {
        validate_budget(topic, MAX_TOPIC_CHARS)?;
    }
    if let Some(location) = record.location.as_deref() {
        validate_budget(location, MAX_LOCATION_CHARS)?;
    }
    if record.topic.as_deref().is_none_or(str::is_empty)
        && record.location.as_deref().is_none_or(str::is_empty)
    {
        return Err(crate::error::AppError::msg(ERROR_EVIDENCE_INSUFFICIENT));
    }
    parse_rfc3339(&record.published_at)?;
    Ok(())
}

fn validate_finance(record: &FinanceRecord) -> crate::error::AppResult<()> {
    validate_origin(&record.origin)?;
    validate_required(&record.instrument, ERROR_EVIDENCE_INSUFFICIENT)?;
    validate_required(&record.asset_kind, ERROR_EVIDENCE_INSUFFICIENT)?;
    validate_required(&record.currency, ERROR_EVIDENCE_INSUFFICIENT)?;
    validate_required(&record.as_of, ERROR_EVIDENCE_INSUFFICIENT)?;
    validate_required(&record.delay, ERROR_EVIDENCE_INSUFFICIENT)?;
    validate_required(&record.value, ERROR_EVIDENCE_INSUFFICIENT)?;
    validate_budget(&record.instrument, MAX_INSTRUMENT_CHARS)?;
    validate_budget(&record.asset_kind, MAX_ASSET_KIND_CHARS)?;
    validate_budget(&record.currency, MAX_CURRENCY_CHARS)?;
    validate_budget(&record.delay, MAX_DELAY_CHARS)?;
    validate_budget(&record.value, MAX_VALUE_CHARS)?;
    parse_rfc3339(&record.as_of)?;
    let delay_minutes = parse_delay_minutes(&record.delay)?;
    if delay_minutes > 15 {
        return Err(crate::error::AppError::msg(ERROR_STALE));
    }
    Ok(())
}

fn validate_entertainment(
    record: &EntertainmentRecord,
    requested_at: DateTime<Utc>,
) -> crate::error::AppResult<()> {
    validate_origin(&record.origin)?;
    validate_required(&record.title, ERROR_EVIDENCE_INSUFFICIENT)?;
    validate_required(&record.region, ERROR_EVIDENCE_INSUFFICIENT)?;
    validate_required(&record.channel, ERROR_EVIDENCE_INSUFFICIENT)?;
    validate_required(&record.date, ERROR_EVIDENCE_INSUFFICIENT)?;
    validate_required(&record.checked_at, ERROR_EVIDENCE_INSUFFICIENT)?;
    validate_budget(&record.title, MAX_TITLE_CHARS)?;
    validate_budget(&record.region, MAX_REGION_CHARS)?;
    validate_budget(&record.channel, MAX_CHANNEL_CHARS)?;
    validate_budget(&record.date, MAX_DATE_CHARS)?;
    parse_date_only(&record.date)?;
    validate_not_stale(
        &record.checked_at,
        requested_at,
        Duration::hours(24),
        ERROR_STALE,
    )
}

fn validate_sports_schedule(
    record: &SportsRecord,
    requested_at: DateTime<Utc>,
) -> crate::error::AppResult<()> {
    validate_sports_common(record)?;
    validate_not_stale(
        &record.checked_at,
        requested_at,
        Duration::hours(24),
        ERROR_STALE,
    )
}

fn validate_sports_score(
    record: &SportsRecord,
    requested_at: DateTime<Utc>,
) -> crate::error::AppResult<()> {
    validate_sports_common(record)?;
    validate_not_stale(
        &record.checked_at,
        requested_at,
        Duration::minutes(15),
        ERROR_STALE,
    )
}

fn validate_sports_common(record: &SportsRecord) -> crate::error::AppResult<()> {
    validate_origin(&record.origin)?;
    validate_required(&record.competition, ERROR_EVIDENCE_INSUFFICIENT)?;
    validate_required(&record.start_time, ERROR_EVIDENCE_INSUFFICIENT)?;
    validate_required(&record.status, ERROR_EVIDENCE_INSUFFICIENT)?;
    validate_required(&record.checked_at, ERROR_EVIDENCE_INSUFFICIENT)?;
    validate_budget(&record.competition, MAX_COMPETITION_CHARS)?;
    validate_budget(&record.status, MAX_STATUS_CHARS)?;
    if record.participants.is_empty() {
        return Err(crate::error::AppError::msg(ERROR_EVIDENCE_INSUFFICIENT));
    }
    if record.participants.len() > MAX_PARTICIPANTS {
        return Err(crate::error::AppError::msg(ERROR_FIELD_TOO_LONG));
    }
    for participant in &record.participants {
        validate_required(participant, ERROR_EVIDENCE_INSUFFICIENT)?;
        validate_budget(participant, MAX_PARTICIPANT_CHARS)?;
    }
    if let Some(score) = record.score.as_deref() {
        validate_budget(score, MAX_SCORE_CHARS)?;
    }
    parse_rfc3339(&record.start_time)?;
    parse_rfc3339(&record.checked_at)?;
    Ok(())
}

fn validate_origin(origin: &EvidenceOrigin) -> crate::error::AppResult<()> {
    if !origin.source_url.starts_with("https://") {
        return Err(crate::error::AppError::msg(ERROR_HTTPS_REQUIRED));
    }
    validate_budget(&origin.provider_id, MAX_PROVIDER_ID_CHARS)?;
    validate_budget(&origin.source_url, MAX_SOURCE_URL_CHARS)?;
    validate_budget(&origin.source_title, MAX_SOURCE_TITLE_CHARS)?;
    validate_budget(&origin.observed_at, MAX_TIME_CHARS)?;
    parse_rfc3339(&origin.observed_at)?;
    Ok(())
}

fn validate_required_location(location: &str) -> crate::error::AppResult<()> {
    validate_required(location, ERROR_LOCATION_REQUIRED)?;
    validate_budget(location, MAX_LOCATION_CHARS)
}

fn validate_required(value: &str, code: &str) -> crate::error::AppResult<()> {
    if value.trim().is_empty() {
        Err(crate::error::AppError::msg(code))
    } else {
        Ok(())
    }
}

fn validate_budget(value: &str, max_chars: usize) -> crate::error::AppResult<()> {
    if value.chars().count() > max_chars {
        Err(crate::error::AppError::msg(ERROR_FIELD_TOO_LONG))
    } else {
        Ok(())
    }
}

fn validate_weather_units(units: &str) -> crate::error::AppResult<()> {
    let normalized = units.trim().to_ascii_lowercase();
    let known = matches!(
        normalized.as_str(),
        "c" | "celsius" | "°c" | "f" | "fahrenheit" | "°f"
    );
    if known {
        Ok(())
    } else {
        Err(crate::error::AppError::msg(ERROR_UNKNOWN_UNIT))
    }
}

fn parse_rfc3339(value: &str) -> crate::error::AppResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| crate::error::AppError::msg(ERROR_INVALID_TIME))
}

fn parse_date_only(value: &str) -> crate::error::AppResult<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| crate::error::AppError::msg(ERROR_INVALID_TIME))
}

fn validate_not_stale(
    fact_time: &str,
    requested_at: DateTime<Utc>,
    max_age: Duration,
    stale_code: &str,
) -> crate::error::AppResult<()> {
    let fact_time = parse_rfc3339(fact_time)?;
    let age = requested_at.signed_duration_since(fact_time);
    if age > max_age {
        Err(crate::error::AppError::msg(stale_code))
    } else {
        Ok(())
    }
}

fn parse_delay_minutes(value: &str) -> crate::error::AppResult<i64> {
    let trimmed = value.trim().to_ascii_lowercase();
    let (number_part, unit_part) = if let Some(stripped) = trimmed.strip_suffix("minutes") {
        (stripped, "minutes")
    } else if let Some(stripped) = trimmed.strip_suffix("minute") {
        (stripped, "minute")
    } else if let Some(stripped) = trimmed.strip_suffix("min") {
        (stripped, "min")
    } else if let Some(stripped) = trimmed.strip_suffix('m') {
        (stripped, "m")
    } else if trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        (trimmed.as_str(), "")
    } else {
        return Err(crate::error::AppError::msg(ERROR_UNKNOWN_UNIT));
    };

    if number_part.trim().is_empty() {
        return Err(crate::error::AppError::msg(ERROR_INVALID_TIME));
    }
    if !matches!(unit_part, "" | "minutes" | "minute" | "min" | "m") {
        return Err(crate::error::AppError::msg(ERROR_UNKNOWN_UNIT));
    }
    number_part
        .trim()
        .parse::<i64>()
        .map_err(|_| crate::error::AppError::msg(ERROR_INVALID_TIME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_delay_supports_minutes_units() {
        assert_eq!(parse_delay_minutes("15").unwrap(), 15);
        assert_eq!(parse_delay_minutes("15 minutes").unwrap(), 15);
        assert_eq!(parse_delay_minutes("5m").unwrap(), 5);
        assert_eq!(
            parse_delay_minutes("soon").unwrap_err().to_string(),
            ERROR_UNKNOWN_UNIT
        );
    }
}
