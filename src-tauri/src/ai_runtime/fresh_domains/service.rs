//! Execution service for current-fact domain operations.
//!
//! The service consumes only normalized tool requests and frozen provider
//! snapshots. It never exposes raw provider JSON: provider results are reduced
//! through a whitelist output mapping into Appendix-D DTOs, validated by Task 2,
//! and only those validated DTOs are returned to the caller.

use chrono::{DateTime, Utc};
use serde_json::{Map, Value};

use super::contracts::{
    EntertainmentRecord, EvidenceOrigin, FinanceRecord, FreshDomainRecord, NewsRecord,
    SportsRecord, WeatherRecord,
};
use super::location::{
    first_location_scope, resolve_confirmed_location, ConfirmedLocation, LocationScope,
};
use super::provider::{resolve_domain_provider, DomainProviderRoute};
use super::validation::validate_domain_record;
use crate::ai_runtime::fresh_research_plan::EvidenceGap;
use crate::ai_runtime::mcp_external_tools::{
    load_run_snapshots, provider_is_current, snapshot_contract_is_valid,
    validate_and_map_arguments, DomainOperation, DomainOutputMapping, FrozenMcpToolSnapshot,
    WEB_DOMAIN_READ_CAPABILITY,
};
use crate::ai_runtime::tool_dispatch::{read_global_memories, ToolDispatchContext};
use crate::ai_runtime::web_evidence_broker::{
    collect_initial_run_web_evidence_with_usage, domain_from_url, WebEvidenceBrokerInput,
    WebEvidenceItem,
};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

const ERROR_MAPPING_INVALID: &str = "external_tool_mapping_invalid";
const ERROR_MAPPING_TOO_LARGE: &str = "external_tool_mapping_output_too_large";
const ERROR_SNAPSHOT_INVALID: &str = "external_tool_binding_config_changed";
const ERROR_PROVIDER_CHANGED: &str = "external_tool_provider_config_changed";
const ERROR_EVIDENCE_INSUFFICIENT: &str = "agent_run_fresh_evidence_insufficient";
const ERROR_LOCATION_REQUIRED: &str = "agent_run_location_required";
const MAX_MAPPED_FIELD_CHARS: usize = 4_096;
const MAX_MAPPED_ARRAY_CHARS: usize = 8_192;

/// Normalized current-fact domain request already validated by tool dispatch.
#[derive(Debug, Clone)]
pub(crate) struct FreshDomainRequest {
    pub(crate) tool_name: String,
    pub(crate) operation: DomainOperation,
    pub(crate) args: Value,
    pub(crate) requested_at: DateTime<Utc>,
    /// Evidence gap that motivated the current request. `LocationCoverage`
    /// allows widening city → province → country when the narrower scope
    /// returns no usable evidence.
    pub(crate) location_gap: Option<EvidenceGap>,
}

/// Stateless current-fact domain executor.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct FreshDomainService;

impl FreshDomainService {
    /// Execute one domain operation against the run-frozen MCP mapping or the
    /// generic Web fallback and return only validated Appendix-D records.
    pub(crate) async fn execute(
        &self,
        mut request: FreshDomainRequest,
        context: &ToolDispatchContext<'_>,
    ) -> AppResult<Vec<FreshDomainRecord>> {
        let db = context
            .db
            .ok_or_else(|| AppError::msg("fresh_domain_context_missing_db"))?;
        let memories = read_global_memories(db)?;
        let explicit = explicit_location_from_args(&request.args);
        let confirmed = resolve_confirmed_location(explicit.as_ref(), &memories);
        enforce_location_requirement(request.operation, &confirmed)?;

        let mut current_scope = first_location_scope(&confirmed);
        loop {
            if let Some(scope) = current_scope {
                request.args =
                    with_location_scope(request.operation, &request.args, scope, &confirmed);
            }
            match self.execute_once(&request, context).await {
                Ok(records) => return Ok(records),
                Err(error)
                    if error.to_string() == ERROR_EVIDENCE_INSUFFICIENT
                        && request.location_gap == Some(EvidenceGap::LocationCoverage)
                        && allows_location_widening(request.operation) =>
                {
                    let Some(next_scope) = current_scope.and_then(|scope| scope.next(&confirmed))
                    else {
                        return Err(error);
                    };
                    current_scope = Some(next_scope);
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn execute_once(
        &self,
        request: &FreshDomainRequest,
        context: &ToolDispatchContext<'_>,
    ) -> AppResult<Vec<FreshDomainRecord>> {
        let db = context
            .db
            .ok_or_else(|| AppError::msg("fresh_domain_context_missing_db"))?;

        if let Some(route) = self.frozen_route(db, request, context)? {
            return self.execute_frozen(db, request, context, route).await;
        }

        self.execute_web_fallback(db, request, context).await
    }

    fn frozen_route(
        &self,
        db: &Database,
        request: &FreshDomainRequest,
        context: &ToolDispatchContext<'_>,
    ) -> AppResult<Option<FrozenMcpToolSnapshot>> {
        let Some(run_id) = context.run_id else {
            let route = resolve_domain_provider(db, request.operation, None)?;
            return match route {
                DomainProviderRoute::FrozenMcp(snapshot) => Ok(Some(snapshot)),
                DomainProviderRoute::WebEvidence => Ok(None),
            };
        };
        let snapshots = load_run_snapshots(db, run_id)?;
        let snapshot = snapshots
            .iter()
            .find(|snapshot| {
                snapshot.capability == WEB_DOMAIN_READ_CAPABILITY
                    && snapshot.domain_operation == Some(request.operation)
            })
            .cloned();
        if let Some(snapshot) = snapshot {
            if !snapshot_contract_is_valid(&snapshot) {
                return Err(AppError::msg(ERROR_SNAPSHOT_INVALID));
            }
            if !provider_is_current(db, &snapshot)? {
                return Err(AppError::msg(ERROR_PROVIDER_CHANGED));
            }
            Ok(Some(snapshot))
        } else {
            Ok(None)
        }
    }

    async fn execute_frozen(
        &self,
        db: &Database,
        request: &FreshDomainRequest,
        _context: &ToolDispatchContext<'_>,
        snapshot: FrozenMcpToolSnapshot,
    ) -> AppResult<Vec<FreshDomainRecord>> {
        let mapped_arguments = validate_and_map_arguments(&snapshot, &request.args)?;
        let provider = crate::ai_runtime::capability_resolver::ResolvedCapabilityProvider {
            capability: WEB_DOMAIN_READ_CAPABILITY.into(),
            provider_kind: "mcp".into(),
            profile_id: snapshot.provider_id.clone(),
            tool_name: snapshot.mcp_tool_name.clone(),
            schema_hash: snapshot.binding_config_hash.clone(),
            requires_confirmation: false,
        };
        let frozen_provider =
            crate::ai_runtime::mcp_external_tools::frozen_provider_config(&snapshot);
        let call = crate::ai_runtime::mcp_host_runtime::call_frozen_provider_tool(
            db,
            &provider,
            &frozen_provider,
            mapped_arguments,
            crate::ai_runtime::mcp_host_runtime::McpHostRuntimeOptions {
                request_timeout: std::time::Duration::from_secs(20),
                max_stdout_line_bytes: 64 * 1024,
                max_stderr_bytes: 8 * 1024,
                cwd: None,
                stdio_session_pool: true,
                stdio_session_idle_timeout:
                    crate::ai_runtime::mcp_host_runtime::DEFAULT_STDIO_SESSION_IDLE_TIMEOUT,
            },
        )
        .await?;
        let mapping = snapshot
            .output_mapping
            .as_ref()
            .ok_or_else(|| AppError::msg(ERROR_MAPPING_INVALID))?;
        let mapped_records = extract_mapped_records(mapping, &call.result)?;
        let mut records = Vec::new();
        for mapped in mapped_records {
            let record = build_record(request.operation, &snapshot, mapped)?;
            validate_domain_record(request.operation, request.requested_at, &record)?;
            records.push(record);
        }
        if records.is_empty() {
            return Err(AppError::msg(ERROR_EVIDENCE_INSUFFICIENT));
        }
        Ok(records)
    }

    async fn execute_web_fallback(
        &self,
        db: &Database,
        request: &FreshDomainRequest,
        context: &ToolDispatchContext<'_>,
    ) -> AppResult<Vec<FreshDomainRecord>> {
        let output = collect_initial_run_web_evidence_with_usage(
            db,
            WebEvidenceBrokerInput {
                query: build_web_query(request),
                urls: Vec::new(),
                enabled: context.web_search_enabled,
                max_search_results: 10,
                max_fetches: 2,
                provider_snapshots: Vec::new(),
                provider_selection_frozen: false,
            },
        )
        .await?;
        let mut records = Vec::new();
        for item in output
            .items
            .iter()
            .filter(|item| item.failure_reason.is_none())
        {
            if let Some(record) =
                record_from_web_item(request.operation, item, request.requested_at)
            {
                if validate_domain_record(request.operation, request.requested_at, &record).is_ok()
                {
                    records.push(record);
                }
            }
        }
        if records.is_empty() {
            return Err(AppError::msg(ERROR_EVIDENCE_INSUFFICIENT));
        }
        Ok(records)
    }
}

pub(crate) fn explicit_location_from_args(args: &Value) -> Option<ConfirmedLocation> {
    let city = args
        .get("location")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)?;
    Some(ConfirmedLocation {
        city: Some(city),
        province: None,
        country: None,
    })
}

pub(crate) fn enforce_location_requirement(
    operation: DomainOperation,
    confirmed: &ConfirmedLocation,
) -> AppResult<()> {
    if requires_city(operation) && confirmed.city.is_none() {
        return Err(AppError::msg(ERROR_LOCATION_REQUIRED));
    }
    Ok(())
}

fn requires_city(operation: DomainOperation) -> bool {
    matches!(
        operation,
        DomainOperation::WeatherCurrent
            | DomainOperation::WeatherForecast
            | DomainOperation::EntertainmentNowPlaying
    )
}

pub(crate) fn allows_location_widening(operation: DomainOperation) -> bool {
    matches!(
        operation,
        DomainOperation::NewsSearch | DomainOperation::EntertainmentUpcoming
    )
}

pub(crate) fn with_location_scope(
    operation: DomainOperation,
    args: &Value,
    scope: LocationScope,
    confirmed: &ConfirmedLocation,
) -> Value {
    if !matches!(
        operation,
        DomainOperation::WeatherCurrent
            | DomainOperation::WeatherForecast
            | DomainOperation::NewsSearch
            | DomainOperation::EntertainmentNowPlaying
            | DomainOperation::EntertainmentUpcoming
            | DomainOperation::EntertainmentStreaming
    ) {
        return args.clone();
    }
    let Some(value) = scope.value(confirmed) else {
        return args.clone();
    };
    let mut args = args.clone();
    if let Some(object) = args.as_object_mut() {
        object.insert("location".into(), Value::String(value.to_string()));
    }
    args
}

fn build_web_query(request: &FreshDomainRequest) -> String {
    let mut parts = Vec::new();
    for key in [
        "location",
        "topic",
        "instrument",
        "title",
        "competition",
        "participant",
    ] {
        if let Some(value) = request
            .args
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            parts.push(value.to_string());
        }
    }
    let domain_hint = match request.operation {
        DomainOperation::WeatherCurrent | DomainOperation::WeatherForecast => "天气 天气预报",
        DomainOperation::NewsSearch => "新闻 news",
        DomainOperation::FinanceQuote
        | DomainOperation::FinanceMetrics
        | DomainOperation::FinanceNews => "股票 行情 金融",
        DomainOperation::EntertainmentNowPlaying
        | DomainOperation::EntertainmentUpcoming
        | DomainOperation::EntertainmentStreaming => "电影 上映 流媒体",
        DomainOperation::SportsSchedule | DomainOperation::SportsScore => "比赛 赛程 比分",
    };
    parts.push(domain_hint.to_string());
    let mut query = parts.join(" ");
    query.push(' ');
    query.push_str(request.operation.as_str());
    query.chars().take(360).collect()
}

/// Extract records from a provider JSON value using a whitelist output mapping.
///
/// Only `$`, dot properties and non-negative array subscripts are supported.
/// Strings, numbers and booleans are converted to strings; arrays must contain
/// only scalar values. Missing paths or type mismatches fail closed.
pub(crate) fn extract_mapped_records(
    mapping: &DomainOutputMapping,
    provider_output: &Value,
) -> AppResult<Vec<Map<String, Value>>> {
    let records = read_path(provider_output, &mapping.records_path)?
        .as_array()
        .ok_or_else(|| AppError::msg(ERROR_MAPPING_INVALID))?
        .iter()
        .map(|record| {
            let mut mapped = Map::new();
            for (field, path) in &mapping.fields {
                let value = read_path(record, path)?;
                let converted = scalar_or_scalar_array(value)?;
                let budget = if converted.is_array() {
                    MAX_MAPPED_ARRAY_CHARS
                } else {
                    MAX_MAPPED_FIELD_CHARS
                };
                if converted
                    .as_str()
                    .is_some_and(|text| text.chars().count() > budget)
                    || converted.as_array().is_some_and(|items| {
                        items
                            .iter()
                            .map(|item| item.as_str().unwrap_or_default().chars().count())
                            .sum::<usize>()
                            > budget
                    })
                {
                    return Err(AppError::msg(ERROR_MAPPING_TOO_LARGE));
                }
                mapped.insert(field.clone(), converted);
            }
            Ok(mapped)
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(records)
}

fn read_path<'a>(root: &'a Value, path: &str) -> AppResult<&'a Value> {
    let path = path.trim();
    if path == "$" {
        return Ok(root);
    }
    let rest = path
        .strip_prefix('$')
        .ok_or_else(|| AppError::msg(ERROR_MAPPING_INVALID))?;
    let mut current = root;
    let mut index = 0;
    while index < rest.len() {
        if rest[index..].starts_with('.') {
            index += 1;
            let start = index;
            while index < rest.len() {
                let ch = rest[index..].chars().next().unwrap();
                if ch == '.' || ch == '[' {
                    break;
                }
                index += ch.len_utf8();
            }
            if start == index {
                return Err(AppError::msg(ERROR_MAPPING_INVALID));
            }
            let property = &rest[start..index];
            current = current
                .get(property)
                .ok_or_else(|| AppError::msg(ERROR_MAPPING_INVALID))?;
        } else if rest[index..].starts_with('[') {
            let Some(close) = rest[index..].find(']').map(|offset| index + offset) else {
                return Err(AppError::msg(ERROR_MAPPING_INVALID));
            };
            let digits = &rest[index + 1..close];
            if digits.is_empty() || !digits.chars().all(|ch| ch.is_ascii_digit()) {
                return Err(AppError::msg(ERROR_MAPPING_INVALID));
            }
            let item_index: usize = digits
                .parse()
                .map_err(|_| AppError::msg(ERROR_MAPPING_INVALID))?;
            current = current
                .get(item_index)
                .ok_or_else(|| AppError::msg(ERROR_MAPPING_INVALID))?;
            index = close + 1;
        } else {
            return Err(AppError::msg(ERROR_MAPPING_INVALID));
        }
    }
    Ok(current)
}

fn scalar_or_scalar_array(value: &Value) -> AppResult<Value> {
    match value {
        Value::String(text) => Ok(Value::String(text.clone())),
        Value::Number(number) => Ok(Value::String(number.to_string())),
        Value::Bool(boolean) => Ok(Value::String(boolean.to_string())),
        Value::Array(items) => {
            let mut converted = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::String(text) => converted.push(Value::String(text.clone())),
                    Value::Number(number) => converted.push(Value::String(number.to_string())),
                    Value::Bool(boolean) => converted.push(Value::String(boolean.to_string())),
                    _ => return Err(AppError::msg(ERROR_MAPPING_INVALID)),
                }
            }
            Ok(Value::Array(converted))
        }
        _ => Err(AppError::msg(ERROR_MAPPING_INVALID)),
    }
}

fn build_record(
    operation: DomainOperation,
    snapshot: &FrozenMcpToolSnapshot,
    mapped: Map<String, Value>,
) -> AppResult<FreshDomainRecord> {
    let origin = origin_from_mapped(&mapped, snapshot)?;
    let mut object = mapped.clone();
    for key in [
        "sourceUrl",
        "sourceTitle",
        "observedAt",
        "evidenceId",
        "providerId",
    ] {
        object.remove(key);
    }
    object.insert("origin".into(), serde_json::to_value(origin)?);
    match operation {
        DomainOperation::WeatherCurrent | DomainOperation::WeatherForecast => {
            Ok(FreshDomainRecord::Weather(
                serde_json::from_value(Value::Object(object))
                    .map_err(|_| AppError::msg(ERROR_MAPPING_INVALID))?,
            ))
        }
        DomainOperation::NewsSearch => Ok(FreshDomainRecord::News(
            serde_json::from_value(Value::Object(object))
                .map_err(|_| AppError::msg(ERROR_MAPPING_INVALID))?,
        )),
        DomainOperation::FinanceQuote
        | DomainOperation::FinanceMetrics
        | DomainOperation::FinanceNews => Ok(FreshDomainRecord::Finance(
            serde_json::from_value(Value::Object(object))
                .map_err(|_| AppError::msg(ERROR_MAPPING_INVALID))?,
        )),
        DomainOperation::EntertainmentNowPlaying
        | DomainOperation::EntertainmentUpcoming
        | DomainOperation::EntertainmentStreaming => Ok(FreshDomainRecord::Entertainment(
            serde_json::from_value(Value::Object(object))
                .map_err(|_| AppError::msg(ERROR_MAPPING_INVALID))?,
        )),
        DomainOperation::SportsSchedule | DomainOperation::SportsScore => {
            Ok(FreshDomainRecord::Sports(
                serde_json::from_value(Value::Object(object))
                    .map_err(|_| AppError::msg(ERROR_MAPPING_INVALID))?,
            ))
        }
    }
}

fn origin_from_mapped(
    mapped: &Map<String, Value>,
    snapshot: &FrozenMcpToolSnapshot,
) -> AppResult<EvidenceOrigin> {
    let source_url = required_string(mapped, "sourceUrl")?;
    let observed_at = required_string(mapped, "observedAt")?;
    let source_title = optional_string(mapped, "sourceTitle")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| snapshot.exposed_name.clone());
    Ok(EvidenceOrigin {
        evidence_id: 0,
        provider_id: snapshot.provider_id.clone(),
        source_url,
        source_title,
        observed_at,
    })
}

fn required_string(mapped: &Map<String, Value>, key: &str) -> AppResult<String> {
    mapped
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| AppError::msg(ERROR_MAPPING_INVALID))
}

fn optional_string(mapped: &Map<String, Value>, key: &str) -> Option<String> {
    mapped
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Parse a generic Web evidence item into one domain record when the item
/// exposes enough Appendix-D fields. Records that cannot be parsed are skipped.
fn record_from_web_item(
    operation: DomainOperation,
    item: &WebEvidenceItem,
    requested_at: DateTime<Utc>,
) -> Option<FreshDomainRecord> {
    let origin = EvidenceOrigin {
        evidence_id: 0,
        provider_id: item.provider_id.clone(),
        source_url: item.url.clone(),
        source_title: if item.title.trim().is_empty() {
            item.url.clone()
        } else {
            item.title.clone()
        },
        observed_at: requested_at.to_rfc3339(),
    };
    let record = match operation {
        DomainOperation::NewsSearch => FreshDomainRecord::News(NewsRecord {
            title: item.title.clone(),
            publisher: domain_from_url(&item.url).unwrap_or_else(|| "unknown".into()),
            published_at: item.freshness_label.clone()?,
            topic: Some("web".to_string()),
            location: None,
            origin,
        }),
        DomainOperation::WeatherCurrent => FreshDomainRecord::Weather(WeatherRecord {
            location: String::new(),
            condition: String::new(),
            temperature: String::new(),
            units: String::new(),
            observation_time: None,
            issue_time: None,
            origin,
        }),
        DomainOperation::WeatherForecast => FreshDomainRecord::Weather(WeatherRecord {
            location: String::new(),
            condition: String::new(),
            temperature: String::new(),
            units: String::new(),
            observation_time: None,
            issue_time: None,
            origin,
        }),
        DomainOperation::FinanceQuote
        | DomainOperation::FinanceMetrics
        | DomainOperation::FinanceNews => FreshDomainRecord::Finance(FinanceRecord {
            instrument: String::new(),
            asset_kind: String::new(),
            currency: String::new(),
            as_of: String::new(),
            delay: String::new(),
            value: String::new(),
            origin,
        }),
        DomainOperation::EntertainmentNowPlaying
        | DomainOperation::EntertainmentUpcoming
        | DomainOperation::EntertainmentStreaming => {
            FreshDomainRecord::Entertainment(EntertainmentRecord {
                title: item.title.clone(),
                region: String::new(),
                channel: String::new(),
                date: String::new(),
                checked_at: requested_at.to_rfc3339(),
                origin,
            })
        }
        DomainOperation::SportsSchedule | DomainOperation::SportsScore => {
            FreshDomainRecord::Sports(SportsRecord {
                competition: String::new(),
                participants: Vec::new(),
                start_time: String::new(),
                status: String::new(),
                score: None,
                checked_at: requested_at.to_rfc3339(),
                origin,
            })
        }
    };
    if validate_domain_record(operation, requested_at, &record).is_ok() {
        Some(record)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::mcp_external_tools::DomainOutputMapping;

    fn mapping(records_path: &str, fields: &[(&str, &str)]) -> DomainOutputMapping {
        DomainOutputMapping {
            records_path: records_path.into(),
            fields: fields
                .iter()
                .map(|(key, path)| (key.to_string(), path.to_string()))
                .collect::<std::collections::BTreeMap<_, _>>(),
        }
    }

    #[test]
    fn output_mapping_extracts_scalars_from_whitelisted_paths() {
        let mapping = mapping(
            "$.records",
            &[
                ("location", "$.city"),
                ("temperature", "$.temp"),
                ("units", "$.unit"),
                ("sourceUrl", "$.url"),
                ("observedAt", "$.time"),
            ],
        );
        let output = serde_json::json!({
            "records": [{
                "city": "北京",
                "temp": 31,
                "unit": "C",
                "url": "https://example.com/weather",
                "time": "2026-08-18T07:00:00Z"
            }]
        });

        let records = extract_mapped_records(&mapping, &output).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["temperature"], "31");
    }

    #[test]
    fn output_mapping_rejects_missing_path() {
        let mapping = mapping("$.records", &[("location", "$.city")]);
        let output = serde_json::json!({ "records": [{}] });

        let error = extract_mapped_records(&mapping, &output).unwrap_err();

        assert_eq!(error.to_string(), ERROR_MAPPING_INVALID);
    }

    #[test]
    fn output_mapping_rejects_type_mismatch() {
        let mapping = mapping("$.records", &[("location", "$.city")]);
        let output = serde_json::json!({ "records": [{"city": {"nested": true}}] });

        let error = extract_mapped_records(&mapping, &output).unwrap_err();

        assert_eq!(error.to_string(), ERROR_MAPPING_INVALID);
    }

    #[test]
    fn output_mapping_rejects_oversized_field() {
        let mapping = mapping("$.records", &[("location", "$.city")]);
        let output = serde_json::json!({ "records": [{"city": "x".repeat(5_000)}] });

        let error = extract_mapped_records(&mapping, &output).unwrap_err();

        assert_eq!(error.to_string(), ERROR_MAPPING_TOO_LARGE);
    }

    #[tokio::test]
    async fn web_fallback_returns_insufficient_when_web_is_disabled() {
        use crate::ai_runtime::retrieval_scope::RetrievalScope;
        use crate::ai_runtime::tool_dispatch::ToolDispatchContext;

        let db = Database::open_in_memory().unwrap();
        let retrieval_scope = RetrievalScope::default();
        let available_tool_names: Vec<String> = Vec::new();
        let cold_start_packets: Vec<crate::ai_runtime::ContextPacket> = Vec::new();
        let runtime_documents: Vec<crate::ai_runtime::RuntimeDocumentSnapshot> = Vec::new();
        let ctx = ToolDispatchContext {
            db: Some(&db),
            selected_web_provider_id: None,
            note_path: None,
            file_id: None,
            run_id: None,
            write_target_path: None,
            document_policy: None,
            web_search_enabled: false,
            fresh_fact_policy: None,
            available_tool_names: &available_tool_names,
            max_web_fetches: 0,
            cold_start_packets: &cold_start_packets,
            retrieval_scope: &retrieval_scope,
            runtime_documents: &runtime_documents,
            app_handle: None,
            attachment_count: 0,
            skill_activation_plan: None,
        };
        let request = FreshDomainRequest {
            tool_name: "weather_lookup".into(),
            operation: DomainOperation::WeatherCurrent,
            args: serde_json::json!({ "location": "北京" }),
            requested_at: DateTime::parse_from_rfc3339("2026-08-18T08:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            location_gap: None,
        };

        let error = FreshDomainService
            .execute_web_fallback(&db, &request, &ctx)
            .await
            .unwrap_err();

        assert_eq!(error.to_string(), ERROR_EVIDENCE_INSUFFICIENT);
    }

    #[test]
    fn web_fallback_keeps_only_records_that_pass_validation() {
        let mut valid = crate::ai_runtime::web_evidence_broker::WebEvidenceItem {
            url: "https://example.com/news".into(),
            canonical_url: "https://example.com/news".into(),
            title: "Example News".into(),
            domain: "example.com".into(),
            snippet: "snippet".into(),
            fetched_excerpt: None,
            provider_id: "web.test".into(),
            provider_kind: "mcp".into(),
            cost_class: "free".into(),
            raw_result_hash: "hash".into(),
            extraction_method: "search_snippet".into(),
            trust_level: "external_untrusted".into(),
            retrieval_reason: "web.search".into(),
            search_backend: crate::ai_runtime::WebSearchBackend::Provider,
            source_rank: crate::ai_runtime::WebSourceRank::Unknown,
            freshness_label: Some("2026-08-18T07:00:00Z".into()),
            failure_reason: None,
            conflict_group: None,
            conflict_note: None,
        };
        let requested_at = DateTime::parse_from_rfc3339("2026-08-18T08:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let record = record_from_web_item(DomainOperation::NewsSearch, &valid, requested_at)
            .expect("news item with freshness parses");

        assert!(validate_domain_record(DomainOperation::NewsSearch, requested_at, &record).is_ok());

        valid.freshness_label = None;
        assert!(record_from_web_item(DomainOperation::NewsSearch, &valid, requested_at).is_none());
    }
}
