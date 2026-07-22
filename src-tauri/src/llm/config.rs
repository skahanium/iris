//! Unified LLM routing configuration (`settings.llm_routing`).

use serde::{Deserialize, Serialize};

use crate::ai_types::{
    EndpointFamily, ReasoningAdapter, ReasoningControl, ReasoningMode, ReasoningVisibility,
    ResolvedReasoningRequest,
};
use crate::error::{AppError, AppResult};
use crate::llm::model_catalog::{fallback_model, find_model};
use crate::llm::model_registry::{self, ModelRegistryEntry};
use crate::llm::providers::{api_base, credential_service, requires_base_url};
use crate::storage::db::Database;

pub const SETTINGS_KEY: &str = "llm_routing";
const LEGACY_CUSTOM_BASE: &str = "llm_custom_base_url";
const LEGACY_BASE_URL: &str = "llm_base_url";
const USAGE_LAST_KEY: &str = "llm_usage_last";
/// Hard cap on computed input budget to prevent runaway values for
/// very-large-context models (1M+). 200K tokens is roughly equivalent
/// to a 500-page book and covers virtually all realistic chat contexts.
const MAX_INPUT_BUDGET_TOKENS: usize = 200_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmRoutingConfig {
    pub version: u32,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub providers: std::collections::HashMap<String, ProviderOverride>,
    /// Explicit first choice in the global enabled-model pool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<ModelReference>,
}

#[cfg(test)]
mod model_pool_tests {
    use super::*;

    fn requirements() -> ModelPoolRequirements {
        ModelPoolRequirements {
            context_tokens: 1,
            has_images: false,
            needs_tools: false,
            needs_reasoning: false,
        }
    }

    #[test]
    fn migration_promotes_legacy_routes_and_removes_slot_fields() {
        let mut value = serde_json::json!({
            "schemaVersion": 4,
            "providers": { "deepseek": { "enabledModels": ["a"] } },
            "slots": {
                "fast": { "providerId": "deepseek", "model": "a" },
                "agent_tools": { "providerId": "deepseek", "model": "b" }
            }
        });

        LlmRoutingConfig::migrate(&mut value).unwrap();

        assert_eq!(value["defaultModel"]["modelId"], "a");
        assert_eq!(
            value["providers"]["deepseek"]["enabledModels"],
            serde_json::json!(["a", "b"])
        );
        assert!(value.get("slots").is_none());
        assert!(value.get("slotFailover").is_none());
    }

    #[test]
    fn default_model_is_first_but_capability_filter_remains_hard() {
        let mut routing = deepseek_defaults();
        routing.providers.insert(
            "deepseek".into(),
            ProviderOverride {
                enabled_models: Some(vec!["deepseek-v4-flash".into(), "deepseek-v4-pro".into()]),
                ..Default::default()
            },
        );
        routing.default_model = Some(ModelReference {
            provider_id: "deepseek".into(),
            model_id: "deepseek-v4-flash".into(),
        });

        let route = resolve_model_pool_from_config(&routing, requirements()).unwrap();
        assert_eq!(route.resolved.model, "deepseek-v4-flash");
        assert!(route
            .failover_candidates
            .iter()
            .any(|candidate| candidate.model == "deepseek-v4-pro"));
    }

    #[test]
    fn pool_filters_vision_and_context_budget_without_slot_fallbacks() {
        let mut routing = deepseek_defaults();
        routing.providers.insert(
            "deepseek".into(),
            ProviderOverride {
                enabled_models: Some(vec!["deepseek-v4-flash".into()]),
                ..Default::default()
            },
        );
        routing.providers.insert(
            "openai".into(),
            ProviderOverride {
                enabled_models: Some(vec!["gpt-4o-mini".into()]),
                ..Default::default()
            },
        );
        routing.default_model = Some(ModelReference {
            provider_id: "deepseek".into(),
            model_id: "deepseek-v4-flash".into(),
        });

        let vision = resolve_model_pool_from_config(
            &routing,
            ModelPoolRequirements {
                context_tokens: 1,
                has_images: true,
                needs_tools: false,
                needs_reasoning: false,
            },
        )
        .expect("vision-capable pool candidate");
        assert_eq!(vision.resolved.model, "gpt-4o-mini");

        routing.default_model = Some(ModelReference {
            provider_id: "openai".into(),
            model_id: "gpt-4o-mini".into(),
        });
        let long_context = resolve_model_pool_from_config(
            &routing,
            ModelPoolRequirements {
                context_tokens: 120_000,
                has_images: false,
                needs_tools: false,
                needs_reasoning: false,
            },
        )
        .expect("long-context pool candidate");
        assert_eq!(long_context.resolved.model, "deepseek-v4-flash");
    }

    #[test]
    fn pool_reports_no_capable_model_when_hard_requirement_has_no_match() {
        let mut routing = deepseek_defaults();
        routing.providers.insert(
            "mimo".into(),
            ProviderOverride {
                enabled_models: Some(vec!["MiMo-V2.5-Pro".into()]),
                ..Default::default()
            },
        );

        let error = resolve_model_pool_from_config(
            &routing,
            ModelPoolRequirements {
                context_tokens: 1,
                has_images: false,
                needs_tools: true,
                needs_reasoning: false,
            },
        )
        .expect_err("model without tools must not be selected");
        assert_eq!(error.to_string(), "agent_run_no_capable_model");
    }

    #[test]
    fn openai_reasoning_model_preserves_its_responses_summary_request_for_dispatch() {
        let mut routing = deepseek_defaults();
        routing.providers.insert(
            "openai".into(),
            ProviderOverride {
                enabled_models: Some(vec!["gpt-5".into()]),
                ..Default::default()
            },
        );
        routing.default_model = Some(ModelReference {
            provider_id: "openai".into(),
            model_id: "gpt-5".into(),
        });

        let resolved = resolve_model_pool_from_config(&routing, requirements())
            .expect("OpenAI reasoning route");

        assert!(resolved.resolved.reasoning.requested);
        assert_eq!(
            resolved.resolved.reasoning.adapter,
            ReasoningAdapter::OpenAiResponses
        );
        assert_eq!(resolved.resolved.reasoning.mode, ReasoningMode::Medium);
    }
}

/// Stable reference to one configured model, stored without endpoint or credential data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelReference {
    pub provider_id: String,
    pub model_id: String,
}

fn default_schema_version() -> u32 {
    LlmRoutingConfig::CURRENT_SCHEMA_VERSION
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderOverride {
    #[serde(default, alias = "base_url")]
    pub base_url: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default, alias = "default_model")]
    pub default_model: Option<String>,
    #[serde(default, alias = "enabled_models")]
    pub enabled_models: Option<Vec<String>>,
    #[serde(
        default,
        alias = "model_capabilities",
        deserialize_with = "deserialize_null_map",
        skip_serializing_if = "std::collections::HashMap::is_empty"
    )]
    pub model_capabilities: std::collections::HashMap<String, ModelCapabilityOverride>,
}

fn deserialize_null_map<'de, D, K, V>(
    deserializer: D,
) -> Result<std::collections::HashMap<K, V>, D::Error>
where
    D: serde::Deserializer<'de>,
    K: std::cmp::Eq + std::hash::Hash + Deserialize<'de>,
    V: Deserialize<'de>,
{
    Ok(Option::<std::collections::HashMap<K, V>>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilityOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_adapter: Option<ReasoningAdapter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_control: Option<ReasoningControl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_visibility: Option<ReasoningVisibility>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supported_modes: Option<Vec<ReasoningMode>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_mode: Option<ReasoningMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disable_supported: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_verified_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe_verified_at: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedLlmConfig {
    pub provider_id: String,
    pub model: String,
    pub base_url: String,
    pub thinking: bool,
    #[serde(default)]
    pub reasoning: ResolvedReasoningRequest,
    pub input_budget: usize,
    pub output_budget: u32,
    pub endpoint_family: EndpointFamily,
    /// Capability facts preserved from the selected model profile for dispatch-time filtering.
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_reasoning: bool,
}

impl std::fmt::Debug for ResolvedLlmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedLlmConfig")
            .field("provider_id", &self.provider_id)
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .field("thinking", &self.thinking)
            .field("reasoning", &self.reasoning)
            .field("input_budget", &self.input_budget)
            .field("output_budget", &self.output_budget)
            .field("endpoint_family", &self.endpoint_family)
            .field("supports_streaming", &self.supports_streaming)
            .field("supports_tools", &self.supports_tools)
            .field("supports_vision", &self.supports_vision)
            .field("supports_reasoning", &self.supports_reasoning)
            .finish()
    }
}

/// Scene-free provider requirements for the unified Agent Run control plane.
///
/// New Harness code supplies these facts directly instead of adapting an intent.
#[derive(Debug, Clone, Copy)]
pub struct ModelPoolRequirements {
    pub context_tokens: usize,
    pub has_images: bool,
    pub needs_tools: bool,
    pub needs_reasoning: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedModelPool {
    pub resolved: ResolvedLlmConfig,
    pub failover_candidates: Vec<ResolvedLlmConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmUsageLast {
    pub prompt_cache_hit_tokens: u32,
    pub prompt_cache_miss_tokens: u32,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectivityStatusDto {
    pub llm: LlmConnectivityDto,
    pub search_provider: SearchProviderConnectivityDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_last: Option<LlmUsageLast>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConnectivityDto {
    pub state: String,
    pub provider_id: String,
    pub model: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchProviderConnectivityDto {
    pub configured: bool,
    pub provider_id: Option<String>,
}

impl Default for LlmRoutingConfig {
    fn default() -> Self {
        deepseek_defaults()
    }
}

impl LlmRoutingConfig {
    /// 当前 schema 版本
    const CURRENT_SCHEMA_VERSION: u32 = 5;

    /// 迁移旧版本配置（就地修改传入的 JSON Value）
    pub fn migrate(config: &mut serde_json::Value) -> AppResult<()> {
        let schema_version = config["schemaVersion"].as_u64().unwrap_or(0) as u32;

        if schema_version < 5 {
            migrate_slots_to_model_pool(config);
            config["schemaVersion"] = serde_json::json!(5);
        }

        Ok(())
    }
}

fn migrate_slots_to_model_pool(config: &mut serde_json::Value) {
    let mut routes = Vec::new();
    if let Some(slots) = config.get("slots").and_then(serde_json::Value::as_object) {
        for route in slots.values() {
            routes.push(route.clone());
        }
    }
    if let Some(failovers) = config
        .get("slotFailover")
        .and_then(serde_json::Value::as_object)
    {
        for routes_for_slot in failovers.values() {
            if let Some(items) = routes_for_slot.as_array() {
                routes.extend(items.iter().cloned());
            }
        }
    }

    let default = config
        .get("defaultModel")
        .filter(|value| value.is_object())
        .cloned()
        .or_else(|| {
            config
                .get("slots")
                .and_then(serde_json::Value::as_object)
                .and_then(|slots| slots.get("fast"))
                .and_then(route_to_model_reference)
        })
        .or_else(|| routes.first().and_then(route_to_model_reference));

    let providers = config
        .as_object_mut()
        .and_then(|object| object.get_mut("providers"))
        .and_then(serde_json::Value::as_object_mut);
    if let Some(providers) = providers {
        for route in &routes {
            let Some(reference) = route_to_model_reference(route) else {
                continue;
            };
            let provider = providers
                .entry(
                    reference["providerId"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                )
                .or_insert_with(|| serde_json::json!({}));
            let Some(provider) = provider.as_object_mut() else {
                continue;
            };
            let enabled = provider
                .entry("enabledModels")
                .or_insert_with(|| serde_json::json!([]));
            let Some(enabled) = enabled.as_array_mut() else {
                continue;
            };
            let model = reference["modelId"].clone();
            if !enabled.iter().any(|existing| existing == &model) {
                enabled.push(model);
            }
        }
    }
    if let Some(default) = default {
        config["defaultModel"] = default;
    }
    for key in [
        "slots",
        "slotFailover",
        "scenes",
        "contextStrategy",
        "routingPolicy",
    ] {
        config
            .as_object_mut()
            .expect("routing config is an object")
            .remove(key);
    }
}

fn route_to_model_reference(route: &serde_json::Value) -> Option<serde_json::Value> {
    let provider_id = route
        .get("providerId")
        .or_else(|| route.get("provider_id"))?
        .as_str()?
        .trim();
    let model_id = route.get("model")?.as_str()?.trim();
    if provider_id.is_empty() || model_id.is_empty() {
        return None;
    }
    Some(serde_json::json!({ "providerId": provider_id, "modelId": model_id }))
}

/// Factory defaults aligned with user preference (DeepSeek V4 Flash / Pro).
pub fn deepseek_defaults() -> LlmRoutingConfig {
    LlmRoutingConfig {
        version: 1,
        schema_version: LlmRoutingConfig::CURRENT_SCHEMA_VERSION,
        created_at: Some(chrono::Utc::now().to_rfc3339()),
        updated_at: None,
        providers: std::collections::HashMap::new(),
        default_model: None,
    }
}

fn normalize_routing_json_value(value: &mut serde_json::Value) -> AppResult<bool> {
    if !value.is_object() {
        return Err(AppError::msg(format!(
            "invalid {SETTINGS_KEY} shape: expected object at root"
        )));
    }

    let mut changed = false;
    changed |= normalize_json_map_field(value, "providers")?;
    changed |= normalize_provider_capability_maps(value)?;
    Ok(changed)
}

fn normalize_json_map_field(value: &mut serde_json::Value, field: &str) -> AppResult<bool> {
    let object = value
        .as_object_mut()
        .expect("routing shape checked before map normalization");
    match object.get(field) {
        Some(serde_json::Value::Object(_)) => Ok(false),
        Some(serde_json::Value::Null) => {
            object.insert(field.to_string(), serde_json::json!({}));
            Ok(true)
        }
        None => {
            object.insert(field.to_string(), serde_json::json!({}));
            Ok(matches!(field, "providers"))
        }
        Some(_) => Err(AppError::msg(format!(
            "invalid {SETTINGS_KEY} map field `{field}`: expected object or null"
        ))),
    }
}

fn normalize_provider_capability_maps(value: &mut serde_json::Value) -> AppResult<bool> {
    let providers = value
        .get_mut("providers")
        .and_then(|field| field.as_object_mut())
        .expect("providers map normalized before provider entries");
    let mut changed = false;
    for (provider_id, provider) in providers {
        if provider.is_null() {
            *provider = serde_json::json!({});
            changed = true;
        }
        let Some(row) = provider.as_object_mut() else {
            return Err(AppError::msg(format!(
                "invalid {SETTINGS_KEY} provider `{provider_id}`: expected object or null"
            )));
        };

        let snake = row.remove("model_capabilities");
        let camel = row.remove("modelCapabilities");
        let normalized = match (camel, snake) {
            (Some(value), Some(_)) => {
                changed = true;
                Some(normalized_capability_map(
                    value,
                    provider_id,
                    "modelCapabilities",
                )?)
            }
            (Some(value), None) => Some(normalized_capability_map(
                value,
                provider_id,
                "modelCapabilities",
            )?),
            (None, Some(value)) => {
                changed = true;
                Some(normalized_capability_map(
                    value,
                    provider_id,
                    "model_capabilities",
                )?)
            }
            (None, None) => None,
        };

        if let Some(value) = normalized {
            if value.as_object().is_some_and(|object| object.is_empty()) {
                changed = true;
            } else {
                row.insert("modelCapabilities".into(), value);
            }
        }
    }
    Ok(changed)
}

fn normalized_capability_map(
    value: serde_json::Value,
    provider_id: &str,
    field: &str,
) -> AppResult<serde_json::Value> {
    match value {
        serde_json::Value::Null => Ok(serde_json::json!({})),
        serde_json::Value::Object(_) => Ok(value),
        _ => Err(AppError::msg(format!(
            "invalid {SETTINGS_KEY} provider `{provider_id}` map field `{field}`: expected object or null"
        ))),
    }
}

pub fn load(db: &Database) -> AppResult<LlmRoutingConfig> {
    let raw: Option<String> = db.with_conn(|conn| {
        let result: Result<String, _> = conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            [SETTINGS_KEY],
            |row| row.get(0),
        );
        match result {
            Ok(json) => Ok(Some(json)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })?;

    let mut config = match raw {
        Some(json) => {
            let mut value: serde_json::Value = serde_json::from_str(&json)
                .map_err(|e| AppError::msg(format!("invalid {SETTINGS_KEY} JSON: {e}")))?;
            let mut should_save = normalize_routing_json_value(&mut value)?;
            let before_migrate = value.clone();
            LlmRoutingConfig::migrate(&mut value)
                .map_err(|e| AppError::msg(format!("invalid {SETTINGS_KEY} migration: {e}")))?;
            if value != before_migrate {
                should_save = true;
            }
            let mut config: LlmRoutingConfig =
                serde_json::from_value(value.clone()).map_err(|e| {
                    AppError::msg(format!("invalid {SETTINGS_KEY} map field or schema: {e}"))
                })?;
            sanitize_routing(&mut config);
            if serde_json::to_value(&config)? != value {
                should_save = true;
            }
            if should_save {
                save(db, &config)?;
            }
            return Ok(config);
        }
        None => {
            let migrated = migrate_legacy(db);
            save(db, &migrated)?;
            migrated
        }
    };

    sanitize_routing(&mut config);
    Ok(config)
}

/// Remove unknown providers while preserving Phase3 built-ins and custom endpoints.
fn sanitize_routing(config: &mut LlmRoutingConfig) {
    for provider in config.providers.values_mut() {
        if let Some(model) = provider.default_model.as_deref() {
            provider.default_model = Some(normalize_legacy_model_id(model).into());
        }
        if let Some(models) = provider.enabled_models.as_mut() {
            for model in models.iter_mut() {
                *model = normalize_legacy_model_id(model).into();
            }
            models.sort();
            models.dedup();
        }
    }
    config.providers.retain(|id, _| {
        crate::llm::providers::is_allowed_provider(id)
            || crate::llm::providers::is_custom_provider(id)
    });
    for (id, provider) in config.providers.iter_mut() {
        if !crate::llm::providers::is_custom_provider(id) {
            provider.base_url = None;
        }
    }
    if config.default_model.as_ref().is_some_and(|model| {
        !config
            .providers
            .get(&model.provider_id)
            .and_then(|provider| provider.enabled_models.as_ref())
            .is_some_and(|models| models.iter().any(|id| id == &model.model_id))
    }) {
        config.default_model = None;
    }
}

pub fn save(db: &Database, config: &LlmRoutingConfig) -> AppResult<()> {
    let json = serde_json::to_string(config)?;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![SETTINGS_KEY, json],
        )?;
        Ok(())
    })
}

fn migrate_legacy(db: &Database) -> LlmRoutingConfig {
    let mut config = deepseek_defaults();

    if let Ok(Some(custom)) = read_setting_string(db, LEGACY_CUSTOM_BASE) {
        config
            .providers
            .entry("custom".into())
            .or_default()
            .base_url = Some(custom);
    }

    if let Ok(Some(_base)) = read_setting_string(db, LEGACY_BASE_URL) {
        config
            .providers
            .entry("deepseek".into())
            .or_default()
            .base_url = Some("https://api.deepseek.com".into());
    }

    config
}

fn read_setting_string(db: &Database, key: &str) -> AppResult<Option<String>> {
    db.with_conn(|conn| {
        let result: Result<String, _> =
            conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            });
        match result {
            Ok(raw) => {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if let Some(s) = v.as_str() {
                        return Ok(Some(s.to_string()));
                    }
                    return Ok(None);
                }
                let trimmed = raw.trim().trim_matches('"');
                if trimmed.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(trimmed.to_string()))
                }
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })
}

/// Resolve an ordered route from scene-free capability requirements without reading credentials.
pub fn resolve_model_pool_for_requirements_without_secret(
    db: &Database,
    requirements: ModelPoolRequirements,
) -> AppResult<ResolvedModelPool> {
    let routing = load(db)?;
    model_registry::clear_invalid_vision_validations(db)?;
    let registry = model_registry::entries_from_builtin_and_routing(
        &routing,
        model_registry::list_registry_entries(db)?,
    );
    resolve_model_pool_with_registry(&routing, requirements, &registry)
}

#[cfg(test)]
fn resolve_model_pool_from_config(
    routing: &LlmRoutingConfig,
    requirements: ModelPoolRequirements,
) -> AppResult<ResolvedModelPool> {
    resolve_model_pool_with_registry(routing, requirements, &[])
}

fn resolve_model_pool_with_registry(
    routing: &LlmRoutingConfig,
    requirements: ModelPoolRequirements,
    registry: &[ModelRegistryEntry],
) -> AppResult<ResolvedModelPool> {
    let mut references = enabled_model_references(routing);
    if let Some(default) = routing.default_model.as_ref() {
        if let Some(index) = references.iter().position(|reference| reference == default) {
            let default = references.remove(index);
            references.insert(0, default);
        }
    }

    let mut candidates = Vec::new();
    for reference in references {
        let resolved = resolve_model_reference(routing, &reference, registry);
        if let Ok(resolved) = resolved {
            if resolved_satisfies_requirements(&resolved, requirements) {
                candidates.push(resolved);
            }
        }
    }

    let Some(resolved) = candidates.first().cloned() else {
        return Err(AppError::msg("agent_run_no_capable_model"));
    };
    Ok(ResolvedModelPool {
        resolved,
        failover_candidates: candidates.into_iter().skip(1).collect(),
    })
}

fn enabled_model_references(routing: &LlmRoutingConfig) -> Vec<ModelReference> {
    let mut references = routing
        .providers
        .iter()
        .flat_map(|(provider_id, provider)| {
            provider
                .enabled_models
                .iter()
                .flatten()
                .filter_map(move |model_id| {
                    let model_id = model_id.trim();
                    (!model_id.is_empty()).then(|| ModelReference {
                        provider_id: provider_id.clone(),
                        model_id: model_id.to_string(),
                    })
                })
        })
        .collect::<Vec<_>>();
    references.sort_by(|left, right| {
        left.provider_id
            .cmp(&right.provider_id)
            .then_with(|| left.model_id.cmp(&right.model_id))
    });
    references.dedup();
    references
}

fn resolved_satisfies_requirements(
    resolved: &ResolvedLlmConfig,
    requirements: ModelPoolRequirements,
) -> bool {
    resolved.supports_streaming
        && resolved.input_budget >= requirements.context_tokens
        && (!requirements.has_images || resolved.supports_vision)
        && (!requirements.needs_tools || resolved.supports_tools)
        && (!requirements.needs_reasoning || resolved.supports_reasoning)
}

fn resolve_model_reference(
    routing: &LlmRoutingConfig,
    reference: &ModelReference,
    registry: &[ModelRegistryEntry],
) -> AppResult<ResolvedLlmConfig> {
    let provider_override = routing.providers.get(&reference.provider_id);
    if provider_override.is_none() {
        return Err(AppError::msg(format!(
            "{} 供应商尚未添加",
            provider_label(&reference.provider_id)
        )));
    }
    let custom_base = provider_override.and_then(configured_base_url);
    if requires_base_url(&reference.provider_id) && custom_base.is_none() {
        return Err(AppError::msg(format!(
            "{} 需配置 Base URL 后才能调用",
            provider_label(&reference.provider_id)
        )));
    }
    let base_url = api_base(&reference.provider_id, custom_base);
    let model_spec =
        find_model(&reference.model_id).unwrap_or_else(|| fallback_model(&reference.provider_id));
    let output_budget = model_spec.max_output;
    let input_ratio = if model_spec.context_window >= 256_000 {
        0.85_f32
    } else {
        0.75_f32
    };
    let input_budget = ((model_spec.context_window as f32) * input_ratio) as usize;
    let input_budget = input_budget.saturating_sub(output_budget as usize);
    let input_budget = input_budget.min(MAX_INPUT_BUDGET_TOKENS);
    let supports_vision = model_spec.supports_vision
        || registry.iter().any(|entry| {
            entry.provider_id == reference.provider_id
                && entry.model_id == reference.model_id
                && entry
                    .vision_verified_at
                    .as_deref()
                    .is_some_and(|timestamp| timestamp != "built_in")
        });
    let reasoning_capability = reasoning_capability_for_model(
        routing,
        &reference.provider_id,
        &reference.model_id,
        model_spec,
    );

    Ok(ResolvedLlmConfig {
        provider_id: reference.provider_id.clone(),
        model: reference.model_id.clone(),
        base_url,
        thinking: false,
        reasoning: default_dispatch_reasoning(&reasoning_capability),
        input_budget,
        output_budget,
        endpoint_family: model_spec.endpoint_family,
        supports_streaming: model_spec.supports_streaming,
        supports_tools: model_spec.supports_tools,
        supports_vision,
        supports_reasoning: reasoning_capability.can_request(),
    })
}

#[derive(Debug, Clone)]
struct ReasoningCapability {
    adapter: ReasoningAdapter,
    control: ReasoningControl,
    visibility: ReasoningVisibility,
    supported_modes: Vec<ReasoningMode>,
    default_mode: ReasoningMode,
    disable_supported: bool,
}

impl ReasoningCapability {
    fn none() -> Self {
        Self {
            adapter: ReasoningAdapter::None,
            control: ReasoningControl::None,
            visibility: ReasoningVisibility::HiddenChannel,
            supported_modes: vec![ReasoningMode::Off],
            default_mode: ReasoningMode::Off,
            disable_supported: true,
        }
    }

    fn can_request(&self) -> bool {
        !matches!(self.control, ReasoningControl::None)
            && !matches!(
                self.adapter,
                ReasoningAdapter::None | ReasoningAdapter::OpenAiCompatibleTagStream
            )
    }
}

/// Enable only the provider-native summary surface required by the normal Run
/// process stream. Other providers retain their existing answer-only behavior
/// until they have an equally explicit, safe summary contract.
fn default_dispatch_reasoning(capability: &ReasoningCapability) -> ResolvedReasoningRequest {
    if capability.adapter != ReasoningAdapter::OpenAiResponses || !capability.can_request() {
        return ResolvedReasoningRequest::disabled();
    }
    ResolvedReasoningRequest {
        mode: capability.default_mode,
        adapter: capability.adapter,
        control: capability.control,
        visibility: capability.visibility,
        requested: true,
        isolate_output: true,
    }
}

fn reasoning_capability_for_model(
    routing: &LlmRoutingConfig,
    provider_id: &str,
    model_id: &str,
    model_spec: &crate::llm::model_catalog::ModelCatalogEntry,
) -> ReasoningCapability {
    let inferred = infer_reasoning_capability(provider_id, model_id, model_spec);
    let Some(provider) = routing.providers.get(provider_id) else {
        return inferred;
    };
    let Some(override_row) = provider.model_capabilities.get(model_id) else {
        return inferred;
    };
    ReasoningCapability {
        adapter: override_row.reasoning_adapter.unwrap_or(inferred.adapter),
        control: override_row.reasoning_control.unwrap_or(inferred.control),
        visibility: override_row
            .reasoning_visibility
            .unwrap_or(inferred.visibility),
        supported_modes: override_row
            .supported_modes
            .clone()
            .unwrap_or_else(|| inferred.supported_modes.clone()),
        default_mode: override_row.default_mode.unwrap_or(inferred.default_mode),
        disable_supported: override_row
            .disable_supported
            .unwrap_or(inferred.disable_supported),
    }
}

fn infer_reasoning_capability(
    provider_id: &str,
    model_id: &str,
    model_spec: &crate::llm::model_catalog::ModelCatalogEntry,
) -> ReasoningCapability {
    let provider = provider_id.to_ascii_lowercase();
    let model = model_id.to_ascii_lowercase();
    if let Some(catalog_capability) = model_spec.reasoning_capability() {
        return ReasoningCapability {
            adapter: catalog_capability.adapter,
            control: catalog_capability.control,
            visibility: catalog_capability.visibility,
            supported_modes: catalog_capability.supported_modes.to_vec(),
            default_mode: catalog_capability.default_mode,
            disable_supported: catalog_capability.disable_supported,
        };
    }
    if provider == "deepseek"
        && (model.starts_with("deepseek-")
            || model.contains("reasoner")
            || model_spec.supports_thinking)
    {
        return ReasoningCapability {
            adapter: ReasoningAdapter::DeepSeekReasoningContent,
            control: ReasoningControl::Effort,
            visibility: ReasoningVisibility::HiddenChannel,
            supported_modes: crate::llm::model_catalog::DEEPSEEK_REASONING_MODES.to_vec(),
            default_mode: ReasoningMode::High,
            disable_supported: true,
        };
    }
    if provider == "openai" && is_openai_reasoning_model(&model) {
        return ReasoningCapability {
            adapter: ReasoningAdapter::OpenAiResponses,
            control: ReasoningControl::Effort,
            visibility: ReasoningVisibility::HiddenChannel,
            supported_modes: crate::llm::model_catalog::OPENAI_REASONING_MODES.to_vec(),
            default_mode: ReasoningMode::Medium,
            disable_supported: true,
        };
    }
    if provider == "anthropic" && model_spec.supports_thinking {
        return ReasoningCapability {
            adapter: ReasoningAdapter::AnthropicExtendedThinking,
            control: ReasoningControl::Budget,
            visibility: ReasoningVisibility::HiddenChannel,
            supported_modes: crate::llm::model_catalog::BUDGET_REASONING_MODES.to_vec(),
            default_mode: ReasoningMode::Medium,
            disable_supported: true,
        };
    }
    if (provider == "google" || provider == "gemini") && model_spec.supports_thinking {
        return ReasoningCapability {
            adapter: ReasoningAdapter::GeminiThinkingConfig,
            control: ReasoningControl::Level,
            visibility: ReasoningVisibility::HiddenChannel,
            supported_modes: crate::llm::model_catalog::EFFORT_REASONING_MODES.to_vec(),
            default_mode: ReasoningMode::Medium,
            disable_supported: true,
        };
    }
    if provider == "zhipu" && (model.starts_with("glm-4.5") || model.starts_with("glm-5")) {
        return ReasoningCapability {
            adapter: ReasoningAdapter::GlmThinking,
            control: ReasoningControl::Effort,
            visibility: ReasoningVisibility::HiddenChannel,
            supported_modes: crate::llm::model_catalog::EFFORT_REASONING_MODES.to_vec(),
            default_mode: ReasoningMode::Medium,
            disable_supported: true,
        };
    }
    if provider.contains("qwen") || provider.contains("dashscope") || model.contains("qwen3") {
        return ReasoningCapability {
            adapter: ReasoningAdapter::QwenChatTemplate,
            control: ReasoningControl::Tag,
            visibility: ReasoningVisibility::ContentTag,
            supported_modes: crate::llm::model_catalog::TAG_REASONING_MODES.to_vec(),
            default_mode: ReasoningMode::Auto,
            disable_supported: true,
        };
    }
    if provider == "mimo" && model_spec.supports_thinking {
        return ReasoningCapability {
            adapter: ReasoningAdapter::ProviderSpecificStatic,
            control: ReasoningControl::Switch,
            visibility: ReasoningVisibility::ContentTag,
            supported_modes: crate::llm::model_catalog::SWITCH_REASONING_MODES.to_vec(),
            default_mode: ReasoningMode::On,
            disable_supported: true,
        };
    }
    if is_minimax_reasoning_risk(&provider, &model) {
        return ReasoningCapability {
            adapter: ReasoningAdapter::OpenAiCompatibleTagStream,
            control: ReasoningControl::Tag,
            visibility: ReasoningVisibility::PlainContentRisk,
            supported_modes: crate::llm::model_catalog::TAG_REASONING_MODES.to_vec(),
            default_mode: ReasoningMode::Auto,
            disable_supported: true,
        };
    }
    ReasoningCapability::none()
}

fn is_openai_reasoning_model(model: &str) -> bool {
    model.starts_with("o1")
        || model.starts_with("o3")
        || model.starts_with("o4")
        || model.starts_with("gpt-5")
}

fn is_minimax_reasoning_risk(provider: &str, model: &str) -> bool {
    provider.contains("minimax") || model.contains("minimax") || model == "minimax-m3"
}

fn normalize_legacy_model_id(model: &str) -> &str {
    match model {
        "mimo-vl-7b-experimental" => "MiMo-V2.5-Pro",
        other => other,
    }
}

fn configured_base_url(provider: &ProviderOverride) -> Option<&str> {
    provider
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|base| !base.is_empty())
}

fn provider_label(provider_id: &str) -> &str {
    match provider_id {
        "mimo" => "MiMo",
        id if crate::llm::providers::is_custom_provider(id) => "Custom",
        other => other,
    }
}

pub fn resolve_for_provider_without_secret(
    db: &Database,
    provider_id: &str,
    model: Option<&str>,
) -> AppResult<ResolvedLlmConfig> {
    let routing = load(db)?;
    if !routing.providers.contains_key(provider_id) {
        return Err(AppError::msg(format!(
            "{} 供应商尚未添加",
            provider_label(provider_id)
        )));
    }
    let model_id = model.map(|s| s.to_string()).unwrap_or_else(|| {
        routing
            .providers
            .get(provider_id)
            .and_then(|p| p.default_model.clone())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| fallback_model(provider_id).id.to_string())
    });

    let provider_override = routing.providers.get(provider_id);
    let custom_base = provider_override.and_then(configured_base_url);
    if requires_base_url(provider_id) && custom_base.is_none() {
        return Err(AppError::msg(format!(
            "{} 需配置 Base URL 后才能调用",
            provider_label(provider_id)
        )));
    }
    let base_url = api_base(provider_id, custom_base);
    let model_spec = find_model(&model_id).unwrap_or_else(|| fallback_model(provider_id));

    let reasoning_capability =
        reasoning_capability_for_model(&routing, provider_id, &model_id, model_spec);
    let resolved = ResolvedLlmConfig {
        provider_id: provider_id.to_string(),
        model: model_id,
        base_url,
        thinking: false,
        reasoning: default_dispatch_reasoning(&reasoning_capability),
        input_budget: (model_spec.context_window as f32 * 0.75) as usize,
        output_budget: model_spec.max_output,
        endpoint_family: model_spec.endpoint_family,
        supports_streaming: model_spec.supports_streaming,
        supports_tools: model_spec.supports_tools,
        supports_vision: model_spec.supports_vision,
        supports_reasoning: reasoning_capability.can_request(),
    };
    Ok(resolved)
}

fn llm_credential_available_for_status(_db: &Database, service: &str) -> AppResult<bool> {
    crate::credentials::credential_available(service)
}

pub fn connectivity_status(db: &Database) -> AppResult<ConnectivityStatusDto> {
    let route_result = resolve_model_pool_for_requirements_without_secret(
        db,
        ModelPoolRequirements {
            context_tokens: 0,
            has_images: false,
            needs_tools: false,
            needs_reasoning: false,
        },
    );
    let selected_web_provider =
        crate::ai_runtime::mcp_runtime_registry::resolve_selected_web_search_provider(db).ok();
    let search_provider = SearchProviderConnectivityDto {
        configured: selected_web_provider.is_some(),
        provider_id: selected_web_provider.map(|provider| provider.id),
    };
    let usage_last = read_usage_last(db)?;

    let resolved = match route_result {
        Ok(route) => route.resolved,
        Err(err) => {
            return Ok(ConnectivityStatusDto {
                llm: LlmConnectivityDto {
                    state: "misconfigured".into(),
                    provider_id: String::new(),
                    model: String::new(),
                    message: err.to_string(),
                },
                search_provider,
                usage_last,
            });
        }
    };
    let llm_configured =
        llm_credential_available_for_status(db, &credential_service(&resolved.provider_id))?;
    let llm_state =
        if crate::llm::providers::requires_api_key(&resolved.provider_id) && !llm_configured {
            "missing_key"
        } else if resolved.model.is_empty() {
            "misconfigured"
        } else {
            "ready"
        };

    let message = match llm_state {
        "missing_key" => format!("请配置 {} 的 API Key", resolved.provider_id),
        "misconfigured" => "模型池未配置有效模型".into(),
        _ => format!("{} / {}", resolved.provider_id, resolved.model),
    };

    Ok(ConnectivityStatusDto {
        llm: LlmConnectivityDto {
            state: llm_state.into(),
            provider_id: resolved.provider_id,
            model: resolved.model,
            message,
        },
        search_provider,
        usage_last,
    })
}

pub fn read_usage_last(db: &Database) -> AppResult<Option<LlmUsageLast>> {
    read_setting_string(db, USAGE_LAST_KEY).and_then(|opt| {
        opt.map(|s| {
            serde_json::from_str(&s).map_err(|e| AppError::msg(format!("invalid usage: {e}")))
        })
        .transpose()
    })
}
