//! Contract tests for current-fact domain DTOs and deterministic validation.

use chrono::{DateTime, Utc};

use super::contracts::{
    DomainOperation, EntertainmentRecord, EvidenceOrigin, FinanceRecord, FreshDomainRecord,
    NewsRecord, SportsRecord, WeatherRecord,
};
use super::location::{
    first_location_scope, resolve_confirmed_location, AiMemory, ConfirmedLocation, LocationScope,
};
use super::service::{allows_location_widening, with_location_scope};
use super::validation::validate_domain_record;

const REQUESTED_AT: &str = "2026-08-18T08:00:00Z";
const ERROR_DOMAIN_INVALID: &str = "agent_run_fresh_domain_invalid";
const ERROR_EVIDENCE_INSUFFICIENT: &str = "agent_run_fresh_evidence_insufficient";
const ERROR_HTTPS_REQUIRED: &str = "agent_run_fresh_https_required";
const ERROR_STALE: &str = "agent_run_fresh_evidence_stale";
const ERROR_LOCATION_REQUIRED: &str = "agent_run_location_required";
const ERROR_UNKNOWN_UNIT: &str = "agent_run_fresh_unknown_unit";

fn requested_at() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(REQUESTED_AT)
        .expect("fixed requested_at fixture")
        .with_timezone(&Utc)
}

fn origin(evidence_id: i64) -> EvidenceOrigin {
    EvidenceOrigin {
        evidence_id,
        provider_id: "fixture-provider".to_string(),
        source_url: "https://example.com/source".to_string(),
        source_title: "Fixture Source".to_string(),
        observed_at: REQUESTED_AT.to_string(),
    }
}

fn weather_current_fixture() -> WeatherRecord {
    WeatherRecord {
        location: "北京".to_string(),
        condition: "晴".to_string(),
        temperature: "31".to_string(),
        units: "C".to_string(),
        observation_time: Some("2026-08-18T07:00:00Z".to_string()),
        issue_time: None,
        origin: origin(1),
    }
}

fn weather_forecast_fixture() -> WeatherRecord {
    WeatherRecord {
        location: "北京".to_string(),
        condition: "晴".to_string(),
        temperature: "30".to_string(),
        units: "C".to_string(),
        observation_time: None,
        issue_time: Some("2026-08-17T20:00:00Z".to_string()),
        origin: origin(2),
    }
}

fn news_fixture() -> NewsRecord {
    NewsRecord {
        title: "示例新闻".to_string(),
        publisher: "示例媒体".to_string(),
        published_at: "2026-08-18T07:00:00Z".to_string(),
        topic: Some("科技".to_string()),
        location: None,
        origin: origin(3),
    }
}

fn finance_fixture() -> FinanceRecord {
    FinanceRecord {
        instrument: "AAPL".to_string(),
        asset_kind: "equity".to_string(),
        currency: "USD".to_string(),
        as_of: "2026-08-17T20:00:00Z".to_string(),
        delay: "15 minutes".to_string(),
        value: "234.56".to_string(),
        origin: origin(4),
    }
}

fn entertainment_fixture() -> EntertainmentRecord {
    EntertainmentRecord {
        title: "电影A".to_string(),
        region: "US".to_string(),
        channel: "Streamer".to_string(),
        date: "2026-08-20".to_string(),
        checked_at: "2026-08-18T07:00:00Z".to_string(),
        origin: origin(5),
    }
}

fn sports_fixture(evidence_id: i64, checked_at: &str) -> SportsRecord {
    SportsRecord {
        competition: "NBA".to_string(),
        participants: vec!["Team A".to_string(), "Team B".to_string()],
        start_time: "2026-08-18T12:00:00Z".to_string(),
        status: "live".to_string(),
        score: Some("1-0".to_string()),
        checked_at: checked_at.to_string(),
        origin: origin(evidence_id),
    }
}

fn origin_ref(record: &FreshDomainRecord) -> &EvidenceOrigin {
    match record {
        FreshDomainRecord::Weather(record) => &record.origin,
        FreshDomainRecord::News(record) => &record.origin,
        FreshDomainRecord::Finance(record) => &record.origin,
        FreshDomainRecord::Entertainment(record) => &record.origin,
        FreshDomainRecord::Sports(record) => &record.origin,
    }
}

fn origin_mut(record: &mut FreshDomainRecord) -> &mut EvidenceOrigin {
    match record {
        FreshDomainRecord::Weather(record) => &mut record.origin,
        FreshDomainRecord::News(record) => &mut record.origin,
        FreshDomainRecord::Finance(record) => &mut record.origin,
        FreshDomainRecord::Entertainment(record) => &mut record.origin,
        FreshDomainRecord::Sports(record) => &mut record.origin,
    }
}

fn assert_valid(operation: DomainOperation, record: &FreshDomainRecord) {
    let expected_origin = origin_ref(record).clone();
    validate_domain_record(operation, requested_at(), record)
        .expect("minimal fixture must pass deterministic validation");
    assert_eq!(
        origin_ref(record),
        &expected_origin,
        "origin must be preserved"
    );
}

fn assert_error(operation: DomainOperation, record: &FreshDomainRecord, code: &str) {
    let error = validate_domain_record(operation, requested_at(), record)
        .expect_err("fixture must be rejected");
    assert_eq!(error.to_string(), code);
}

fn all_domain_success_records() -> Vec<(DomainOperation, FreshDomainRecord)> {
    vec![
        (
            DomainOperation::WeatherCurrent,
            FreshDomainRecord::Weather(weather_current_fixture()),
        ),
        (
            DomainOperation::WeatherForecast,
            FreshDomainRecord::Weather(weather_forecast_fixture()),
        ),
        (
            DomainOperation::NewsSearch,
            FreshDomainRecord::News(news_fixture()),
        ),
        (
            DomainOperation::FinanceQuote,
            FreshDomainRecord::Finance(finance_fixture()),
        ),
        (
            DomainOperation::FinanceMetrics,
            FreshDomainRecord::Finance(finance_fixture()),
        ),
        (
            DomainOperation::FinanceNews,
            FreshDomainRecord::Finance(finance_fixture()),
        ),
        (
            DomainOperation::EntertainmentNowPlaying,
            FreshDomainRecord::Entertainment(entertainment_fixture()),
        ),
        (
            DomainOperation::EntertainmentUpcoming,
            FreshDomainRecord::Entertainment(entertainment_fixture()),
        ),
        (
            DomainOperation::EntertainmentStreaming,
            FreshDomainRecord::Entertainment(entertainment_fixture()),
        ),
        (
            DomainOperation::SportsSchedule,
            FreshDomainRecord::Sports(sports_fixture(6, "2026-08-18T07:00:00Z")),
        ),
        (
            DomainOperation::SportsScore,
            FreshDomainRecord::Sports(sports_fixture(7, "2026-08-18T07:55:00Z")),
        ),
    ]
}

#[test]
fn weather_current_minimal_record_passes_and_preserves_origin() {
    let record = FreshDomainRecord::Weather(weather_current_fixture());
    assert_valid(DomainOperation::WeatherCurrent, &record);
}

#[test]
fn weather_forecast_minimal_record_passes_and_preserves_origin() {
    let record = FreshDomainRecord::Weather(weather_forecast_fixture());
    assert_valid(DomainOperation::WeatherForecast, &record);
}

#[test]
fn news_search_minimal_record_passes_and_preserves_origin() {
    let record = FreshDomainRecord::News(news_fixture());
    assert_valid(DomainOperation::NewsSearch, &record);
}

#[test]
fn finance_quote_minimal_record_passes_and_preserves_origin() {
    let record = FreshDomainRecord::Finance(finance_fixture());
    assert_valid(DomainOperation::FinanceQuote, &record);
}

#[test]
fn finance_metrics_minimal_record_passes_and_preserves_origin() {
    let record = FreshDomainRecord::Finance(finance_fixture());
    assert_valid(DomainOperation::FinanceMetrics, &record);
}

#[test]
fn finance_news_minimal_record_passes_and_preserves_origin() {
    let record = FreshDomainRecord::Finance(finance_fixture());
    assert_valid(DomainOperation::FinanceNews, &record);
}

#[test]
fn entertainment_now_playing_minimal_record_passes_and_preserves_origin() {
    let record = FreshDomainRecord::Entertainment(entertainment_fixture());
    assert_valid(DomainOperation::EntertainmentNowPlaying, &record);
}

#[test]
fn entertainment_upcoming_minimal_record_passes_and_preserves_origin() {
    let record = FreshDomainRecord::Entertainment(entertainment_fixture());
    assert_valid(DomainOperation::EntertainmentUpcoming, &record);
}

#[test]
fn entertainment_streaming_minimal_record_passes_and_preserves_origin() {
    let record = FreshDomainRecord::Entertainment(entertainment_fixture());
    assert_valid(DomainOperation::EntertainmentStreaming, &record);
}

#[test]
fn sports_schedule_minimal_record_passes_and_preserves_origin() {
    let record = FreshDomainRecord::Sports(sports_fixture(6, "2026-08-18T07:00:00Z"));
    assert_valid(DomainOperation::SportsSchedule, &record);
}

#[test]
fn sports_score_minimal_record_passes_and_preserves_origin() {
    let record = FreshDomainRecord::Sports(sports_fixture(7, "2026-08-18T07:55:00Z"));
    assert_valid(DomainOperation::SportsScore, &record);
}

#[test]
fn weather_observation_older_than_three_hours_is_rejected() {
    let mut record = weather_current_fixture();
    record.observation_time = Some("2026-08-18T04:00:00Z".to_string());
    assert_error(
        DomainOperation::WeatherCurrent,
        &FreshDomainRecord::Weather(record),
        ERROR_STALE,
    );
}

#[test]
fn weather_forecast_issue_older_than_twelve_hours_is_rejected() {
    let mut record = weather_forecast_fixture();
    record.issue_time = Some("2026-08-17T19:00:00Z".to_string());
    assert_error(
        DomainOperation::WeatherForecast,
        &FreshDomainRecord::Weather(record),
        ERROR_STALE,
    );
}

#[test]
fn news_without_published_at_is_rejected() {
    let mut record = news_fixture();
    record.published_at.clear();
    assert_error(
        DomainOperation::NewsSearch,
        &FreshDomainRecord::News(record),
        ERROR_EVIDENCE_INSUFFICIENT,
    );
}

#[test]
fn finance_without_currency_or_as_of_is_rejected() {
    let mut record = finance_fixture();
    record.currency.clear();
    record.as_of.clear();
    assert_error(
        DomainOperation::FinanceQuote,
        &FreshDomainRecord::Finance(record),
        ERROR_EVIDENCE_INSUFFICIENT,
    );
}

#[test]
fn entertainment_without_region_channel_or_date_is_rejected() {
    let mut record = entertainment_fixture();
    record.region.clear();
    record.channel.clear();
    record.date.clear();
    assert_error(
        DomainOperation::EntertainmentNowPlaying,
        &FreshDomainRecord::Entertainment(record),
        ERROR_EVIDENCE_INSUFFICIENT,
    );
}

#[test]
fn live_score_checked_at_older_than_fifteen_minutes_is_rejected() {
    let record = sports_fixture(7, "2026-08-18T07:30:00Z");
    assert_error(
        DomainOperation::SportsScore,
        &FreshDomainRecord::Sports(record),
        ERROR_STALE,
    );
}

#[test]
fn all_domain_records_require_https_source() {
    for (operation, mut record) in all_domain_success_records() {
        origin_mut(&mut record).source_url = "http://example.com/source".to_string();
        assert_error(operation, &record, ERROR_HTTPS_REQUIRED);
    }
}

#[test]
fn operation_variant_mismatch_is_rejected() {
    let record = FreshDomainRecord::Weather(weather_current_fixture());
    assert_error(DomainOperation::NewsSearch, &record, ERROR_DOMAIN_INVALID);
}

#[test]
fn weather_without_city_returns_location_required() {
    let mut record = weather_current_fixture();
    record.location.clear();
    assert_error(
        DomainOperation::WeatherCurrent,
        &FreshDomainRecord::Weather(record),
        ERROR_LOCATION_REQUIRED,
    );
}

#[test]
fn weather_unknown_unit_is_rejected() {
    let mut record = weather_current_fixture();
    record.units = "banana".to_string();
    assert_error(
        DomainOperation::WeatherCurrent,
        &FreshDomainRecord::Weather(record),
        ERROR_UNKNOWN_UNIT,
    );
}

#[test]
fn entertainment_checked_at_older_than_twenty_four_hours_is_rejected() {
    let mut record = entertainment_fixture();
    record.checked_at = "2026-08-17T07:00:00Z".to_string();
    assert_error(
        DomainOperation::EntertainmentStreaming,
        &FreshDomainRecord::Entertainment(record),
        ERROR_STALE,
    );
}

#[test]
fn confirmed_location_explicit_overrides_memory() {
    let explicit = ConfirmedLocation {
        city: Some("上海".to_string()),
        province: Some("上海".to_string()),
        country: Some("中国".to_string()),
    };
    let memories = vec![
        AiMemory {
            key: "location.city".to_string(),
            content: "北京".to_string(),
            scope: "global".to_string(),
        },
        AiMemory {
            key: "location.province".to_string(),
            content: "广东".to_string(),
            scope: "global".to_string(),
        },
        AiMemory {
            key: "location.country".to_string(),
            content: "日本".to_string(),
            scope: "global".to_string(),
        },
    ];

    let resolved = resolve_confirmed_location(Some(&explicit), &memories);

    assert_eq!(resolved.city.as_deref(), Some("上海"));
    assert_eq!(resolved.province.as_deref(), Some("上海"));
    assert_eq!(resolved.country.as_deref(), Some("中国"));

    let partial = ConfirmedLocation {
        city: Some("深圳".to_string()),
        province: None,
        country: None,
    };
    let resolved = resolve_confirmed_location(Some(&partial), &memories);

    assert_eq!(resolved.city.as_deref(), Some("深圳"));
    assert_eq!(resolved.province.as_deref(), Some("广东"));
    assert_eq!(resolved.country.as_deref(), Some("日本"));
}

#[test]
fn confirmed_location_ignores_vault_web_ip_and_similar_keys() {
    let memories = vec![
        AiMemory {
            key: "location.city".to_string(),
            content: "上海".to_string(),
            scope: "global".to_string(),
        },
        AiMemory {
            key: "location.city".to_string(),
            content: "北京".to_string(),
            scope: "vault:abc".to_string(),
        },
        AiMemory {
            key: "web.city".to_string(),
            content: "广州".to_string(),
            scope: "global".to_string(),
        },
        AiMemory {
            key: "ip.city".to_string(),
            content: "深圳".to_string(),
            scope: "global".to_string(),
        },
        AiMemory {
            key: "location.city_name".to_string(),
            content: "杭州".to_string(),
            scope: "global".to_string(),
        },
        AiMemory {
            key: "location".to_string(),
            content: "成都".to_string(),
            scope: "global".to_string(),
        },
    ];

    let resolved = resolve_confirmed_location(None, &memories);

    assert_eq!(resolved.city.as_deref(), Some("上海"));
    assert_eq!(resolved.province, None);
    assert_eq!(resolved.country, None);

    let vault_only = resolve_confirmed_location(
        None,
        &[AiMemory {
            key: "location.city".to_string(),
            content: "北京".to_string(),
            scope: "vault:abc".to_string(),
        }],
    );
    assert_eq!(vault_only.city, None);
}

#[test]
fn location_scope_widens_city_then_province_then_country() {
    let confirmed = ConfirmedLocation {
        city: Some("上海".to_string()),
        province: Some("江苏".to_string()),
        country: Some("中国".to_string()),
    };

    assert_eq!(first_location_scope(&confirmed), Some(LocationScope::City));
    assert_eq!(
        LocationScope::City.next(&confirmed),
        Some(LocationScope::Province)
    );
    assert_eq!(
        LocationScope::Province.next(&confirmed),
        Some(LocationScope::Country)
    );
    assert_eq!(LocationScope::Country.next(&confirmed), None);

    let province_only = ConfirmedLocation {
        city: None,
        province: Some("广东".to_string()),
        country: Some("中国".to_string()),
    };
    assert_eq!(
        first_location_scope(&province_only),
        Some(LocationScope::Province)
    );
    let args = with_location_scope(
        DomainOperation::NewsSearch,
        &serde_json::json!({ "topic": "科技" }),
        LocationScope::Province,
        &province_only,
    );
    assert_eq!(args["location"], "广东");

    assert!(allows_location_widening(DomainOperation::NewsSearch));
    assert!(allows_location_widening(
        DomainOperation::EntertainmentUpcoming
    ));
    assert!(!allows_location_widening(DomainOperation::WeatherCurrent));
}
