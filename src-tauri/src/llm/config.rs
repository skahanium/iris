//! Unified LLM routing configuration (`settings.llm_routing`).

use serde::{Deserialize, Serialize};

use crate::ai_types::{
    AgentIntent, AiScene, CapabilityRouteSummary, CapabilitySlot, EndpointFamily, ProviderConfig,
    ReasoningAdapter, ReasoningControl, ReasoningMode, ReasoningVisibility,
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
    #[serde(default)]
    pub slots: std::collections::HashMap<String, SlotRoute>,
    #[serde(default, skip_serializing)]
    pub scenes: std::collections::HashMap<String, SceneRoute>,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub context_strategy: std::collections::HashMap<String, ContextStrategy>,
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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningSlotConfig {
    #[serde(default)]
    pub mode: ReasoningMode,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneRoute {
    #[serde(alias = "provider_id")]
    pub provider_id: String,
    pub model: String,
    #[serde(default)]
    pub thinking: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningSlotConfig>,
}

pub type SlotRoute = SceneRoute;

pub use crate::ai_types::ContextStrategy;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedLlmConfig {
    pub provider_id: String,
    pub model: String,
    pub base_url: String,
    #[serde(skip)]
    pub api_key: Option<String>,
    pub thinking: bool,
    #[serde(default)]
    pub reasoning: ResolvedReasoningRequest,
    pub input_budget: usize,
    pub output_budget: u32,
    pub context_strategy: ContextStrategy,
    pub endpoint_family: EndpointFamily,
}

impl std::fmt::Debug for ResolvedLlmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedLlmConfig")
            .field("provider_id", &self.provider_id)
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .field("api_key", &"[REDACTED]")
            .field("thinking", &self.thinking)
            .field("reasoning", &self.reasoning)
            .field("input_budget", &self.input_budget)
            .field("output_budget", &self.output_budget)
            .field("context_strategy", &self.context_strategy)
            .field("endpoint_family", &self.endpoint_family)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyPreference {
    ExternalAllowed,
    LocalOnly,
}

#[derive(Debug, Clone, Copy)]
pub struct CapabilityRouteInput {
    pub intent: AgentIntent,
    pub context_tokens: usize,
    pub has_images: bool,
    pub needs_tools: bool,
    pub needs_reasoning: bool,
    pub privacy_preference: PrivacyPreference,
}

#[derive(Debug, Clone)]
pub struct ResolvedCapabilityRoute {
    pub resolved: ResolvedLlmConfig,
    pub summary: CapabilityRouteSummary,
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
    pub scene: String,
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
    const CURRENT_SCHEMA_VERSION: u32 = 4;

    /// 迁移旧版本配置（就地修改传入的 JSON Value）
    pub fn migrate(config: &mut serde_json::Value) -> AppResult<()> {
        let schema_version = config["schemaVersion"].as_u64().unwrap_or(0) as u32;

        if schema_version < 1 {
            // v0 → v1：补充 schemaVersion、createdAt
            config["schemaVersion"] = serde_json::json!(1);
            if config["createdAt"].is_null() {
                config["createdAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
            }
        }

        if schema_version < 2 {
            let slots = slots_from_legacy_scenes(config);
            config["slots"] = serde_json::json!(slots);
            config["schemaVersion"] = serde_json::json!(2);
        }

        if schema_version < 3 {
            remove_legacy_default_slot_bindings(config);
            remove_builtin_base_url_overrides(config);
            config["schemaVersion"] = serde_json::json!(3);
        }

        if schema_version < 4 {
            migrate_slot_reasoning(config);
            config["schemaVersion"] = serde_json::json!(4);
        }

        Ok(())
    }
}

fn migrate_slot_reasoning(config: &mut serde_json::Value) {
    for section in ["slots", "scenes"] {
        let Some(routes) = config[section].as_object_mut() else {
            continue;
        };
        for route in routes.values_mut() {
            migrate_route_reasoning(route);
        }
    }
}

fn migrate_route_reasoning(route: &mut serde_json::Value) {
    let Some(row) = route.as_object_mut() else {
        return;
    };
    if row.get("reasoning").is_some() {
        return;
    }
    let mode = if row
        .get("thinking")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        "auto"
    } else {
        "off"
    };
    row.insert("reasoning".into(), serde_json::json!({ "mode": mode }));
}

fn slots_from_legacy_scenes(
    config: &serde_json::Value,
) -> std::collections::HashMap<String, serde_json::Value> {
    let mut slots: std::collections::HashMap<String, serde_json::Value> =
        std::collections::HashMap::new();
    let scenes = &config["scenes"];
    let mappings = [
        ("fast", "knowledge_lookup"),
        ("writer", "drafting_assist"),
        ("reasoner", "research_synthesis"),
        ("long_context", "exemplar_learning"),
    ];
    for (slot, scene) in mappings {
        if scenes[scene].is_object() {
            slots.insert(slot.to_string(), scenes[scene].clone());
        }
    }
    slots
}

fn remove_legacy_default_slot_bindings(config: &mut serde_json::Value) {
    let Some(slots) = config["slots"].as_object_mut() else {
        return;
    };
    slots.retain(|slot, route| !is_legacy_default_slot_binding(slot, route));
}

fn is_legacy_default_slot_binding(slot: &str, route: &serde_json::Value) -> bool {
    let provider = route
        .get("providerId")
        .or_else(|| route.get("provider_id"))
        .and_then(|value| value.as_str());
    let model = route.get("model").and_then(|value| value.as_str());
    let thinking = route
        .get("thinking")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    matches!(
        (slot, provider, model, thinking),
        ("fast", Some("deepseek"), Some("deepseek-v4-flash"), false)
            | ("writer", Some("deepseek"), Some("deepseek-v4-pro"), false)
            | ("reasoner", Some("deepseek"), Some("deepseek-v4-pro"), true)
            | (
                "long_context",
                Some("deepseek"),
                Some("deepseek-v4-pro"),
                false
            )
            | (
                "agent_tools",
                Some("deepseek"),
                Some("deepseek-v4-pro"),
                true
            )
            | (
                "embedding",
                Some("deepseek"),
                Some("deepseek-v4-flash"),
                false
            )
            | (
                "reranker",
                Some("deepseek"),
                Some("deepseek-v4-flash"),
                false
            )
            | (
                "local_private",
                Some("deepseek"),
                Some("deepseek-v4-flash"),
                false
            )
            | ("vision", Some("mimo"), Some("mimo-v2.5"), false)
    )
}

fn remove_builtin_base_url_overrides(config: &mut serde_json::Value) {
    let Some(providers) = config["providers"].as_object_mut() else {
        return;
    };
    for (provider_id, provider) in providers {
        if crate::llm::providers::is_custom_provider(provider_id) {
            continue;
        }
        if let Some(row) = provider.as_object_mut() {
            row.remove("baseUrl");
            row.remove("base_url");
        }
    }
}

fn merge_legacy_scene_routes(config: &mut LlmRoutingConfig) {
    if config.scenes.is_empty() {
        return;
    }

    let mappings = [
        ("fast", "knowledge_lookup"),
        ("writer", "drafting_assist"),
        ("reasoner", "research_synthesis"),
        ("long_context", "exemplar_learning"),
    ];
    for (slot, scene) in mappings {
        if let Some(route) = config.scenes.get(scene).cloned() {
            config.slots.insert(slot.to_string(), route);
        }
    }
    config.scenes.clear();
}

/// Factory defaults aligned with user preference (DeepSeek V4 Flash / Pro).
pub fn deepseek_defaults() -> LlmRoutingConfig {
    LlmRoutingConfig {
        version: 1,
        schema_version: LlmRoutingConfig::CURRENT_SCHEMA_VERSION,
        created_at: Some(chrono::Utc::now().to_rfc3339()),
        updated_at: None,
        providers: std::collections::HashMap::new(),
        slots: empty_slot_defaults(),
        scenes: std::collections::HashMap::new(),
        context_strategy: std::collections::HashMap::new(),
    }
}

fn empty_slot_defaults() -> std::collections::HashMap<String, SlotRoute> {
    std::collections::HashMap::new()
}

fn normalize_routing_json_value(value: &mut serde_json::Value) -> AppResult<bool> {
    if !value.is_object() {
        return Err(AppError::msg(format!(
            "invalid {SETTINGS_KEY} shape: expected object at root"
        )));
    }

    let mut changed = false;
    for field in ["providers", "slots", "scenes", "contextStrategy"] {
        changed |= normalize_json_map_field(value, field)?;
    }
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
            Ok(matches!(field, "providers" | "slots"))
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
            let mut config: LlmRoutingConfig = serde_json::from_value(value).map_err(|e| {
                AppError::msg(format!("invalid {SETTINGS_KEY} map field or schema: {e}"))
            })?;
            let before_sanitize = serde_json::to_value(&config)?;
            merge_legacy_scene_routes(&mut config);
            sanitize_routing(&mut config);
            ensure_slot_keys(&mut config);
            if serde_json::to_value(&config)? != before_sanitize {
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

    merge_legacy_scene_routes(&mut config);
    sanitize_routing(&mut config);
    ensure_slot_keys(&mut config);
    Ok(config)
}

/// Remove unknown providers while preserving Phase3 built-ins and custom endpoints.
fn sanitize_routing(config: &mut LlmRoutingConfig) {
    config.slots.retain(|_, route| {
        if !crate::llm::providers::is_allowed_provider(&route.provider_id) {
            return false;
        }
        if !config.providers.contains_key(&route.provider_id) {
            return false;
        }
        !route.model.trim().is_empty()
    });
    for route in config.slots.values_mut() {
        route.model = normalize_legacy_model_id(&route.model).into();
        normalize_route_reasoning(route);
    }
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
}

fn normalize_route_reasoning(route: &mut SlotRoute) {
    if route.reasoning.is_none() {
        route.reasoning = Some(ReasoningSlotConfig {
            mode: if route.thinking {
                ReasoningMode::Auto
            } else {
                ReasoningMode::Off
            },
        });
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

fn ensure_slot_keys(config: &mut LlmRoutingConfig) {
    let _ = empty_slot_defaults();
    config
        .slots
        .retain(|_, route| !route.model.trim().is_empty());
}

fn route_input_for_scene(scene: AiScene) -> CapabilityRouteInput {
    let intent = match scene {
        AiScene::KnowledgeLookup => AgentIntent::AskNotes,
        AiScene::DraftingAssist => AgentIntent::Write,
        AiScene::ResearchSynthesis => AgentIntent::Research,
        _ => AgentIntent::Write,
    };
    CapabilityRouteInput {
        intent,
        context_tokens: 0,
        has_images: false,
        needs_tools: false,
        needs_reasoning: matches!(scene, AiScene::ResearchSynthesis),
        privacy_preference: PrivacyPreference::ExternalAllowed,
    }
}

pub fn resolve_capability_route(
    db: &Database,
    input: CapabilityRouteInput,
) -> AppResult<ResolvedCapabilityRoute> {
    let routing = load(db)?;
    model_registry::clear_invalid_vision_validations(db)?;
    let registry = model_registry::entries_from_builtin_and_routing(
        &routing,
        model_registry::list_registry_entries(db)?,
    );
    let mut route = resolve_capability_route_with_registry(&routing, input, &registry)?;
    hydrate_resolved_api_key(db, &mut route.resolved)?;
    Ok(route)
}

#[cfg(test)]
fn resolve_capability_route_from_config(
    routing: &LlmRoutingConfig,
    input: CapabilityRouteInput,
) -> AppResult<ResolvedCapabilityRoute> {
    resolve_capability_route_with_registry(routing, input, &[])
}

fn resolve_capability_route_for_status(
    db: &Database,
    input: CapabilityRouteInput,
) -> AppResult<AppResult<ResolvedCapabilityRoute>> {
    let routing = load(db)?;
    model_registry::clear_invalid_vision_validations(db)?;
    let registry = model_registry::entries_from_builtin_and_routing(
        &routing,
        model_registry::list_registry_entries(db)?,
    );
    Ok(resolve_capability_route_with_registry(
        &routing, input, &registry,
    ))
}

fn resolve_capability_route_with_registry(
    routing: &LlmRoutingConfig,
    input: CapabilityRouteInput,
    registry: &[ModelRegistryEntry],
) -> AppResult<ResolvedCapabilityRoute> {
    let requested_slot = requested_slot(input);
    let fallback_chain = fallback_chain_for(requested_slot);
    let mut last_error = None;

    for slot in &fallback_chain {
        let key = slot_key(*slot);
        let Some(route) = routing.slots.get(key) else {
            continue;
        };
        if !route_satisfies_slot(route, *slot, input.privacy_preference, registry) {
            continue;
        }

        // Try to resolve this slot (checks base URL, API key, model spec).
        match resolve_route(routing, *slot, route.clone()) {
            Ok(resolved) => {
                let degraded = *slot != requested_slot;
                let reason = if degraded {
                    format!(
                        "Requested {requested_slot:?} but selected {slot:?} via fallback because the requested slot is unavailable or misconfigured."
                    )
                } else {
                    format!("Selected {:?} for {:?}.", slot, input.intent)
                };
                return Ok(ResolvedCapabilityRoute {
                    summary: CapabilityRouteSummary {
                        slot: *slot,
                        provider_id: route.provider_id.clone(),
                        model: route.model.clone(),
                        fallback_chain,
                        reason,
                        probe_status: "unknown".into(),
                        degraded,
                    },
                    resolved,
                });
            }
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        AppError::msg(format!(
            "{} 能力槽未配置可用模型",
            slot_display_name(requested_slot)
        ))
    }))
}

fn resolve_route(
    routing: &LlmRoutingConfig,
    slot: CapabilitySlot,
    route: SlotRoute,
) -> AppResult<ResolvedLlmConfig> {
    let provider_override = routing.providers.get(&route.provider_id);
    if provider_override.is_none() {
        return Err(AppError::msg(format!(
            "{} 供应商尚未添加",
            provider_label(&route.provider_id)
        )));
    }
    let custom_base = provider_override.and_then(configured_base_url);
    if requires_base_url(&route.provider_id) && custom_base.is_none() {
        return Err(AppError::msg(format!(
            "{} 需配置 Base URL 后才能调用",
            provider_label(&route.provider_id)
        )));
    }
    let base_url = api_base(&route.provider_id, custom_base);
    let model_spec = find_model(&route.model).unwrap_or_else(|| fallback_model(&route.provider_id));
    let reasoning = resolve_reasoning_request(routing, slot, &route, model_spec);
    let thinking = reasoning.requested;
    let output_budget = model_spec.max_output;
    let input_ratio = if model_spec.context_window >= 256_000 {
        0.85_f32
    } else {
        0.5_f32
    };
    let input_budget = ((model_spec.context_window as f32) * input_ratio) as usize;
    let input_budget = input_budget.saturating_sub(output_budget as usize);
    let context_strategy = if slot == CapabilitySlot::LongContext {
        ContextStrategy::LongContext
    } else {
        ContextStrategy::Hybrid
    };

    Ok(ResolvedLlmConfig {
        provider_id: route.provider_id,
        model: route.model,
        base_url,
        api_key: None,
        thinking,
        reasoning,
        input_budget,
        output_budget,
        context_strategy,
        endpoint_family: model_spec.endpoint_family,
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

    fn needs_isolation(&self) -> bool {
        !matches!(self.visibility, ReasoningVisibility::HiddenChannel)
            || matches!(
                self.adapter,
                ReasoningAdapter::DeepSeekReasoningContent
                    | ReasoningAdapter::AnthropicExtendedThinking
                    | ReasoningAdapter::GeminiThinkingConfig
                    | ReasoningAdapter::OpenAiResponses
                    | ReasoningAdapter::GlmThinking
            )
    }
}

fn resolve_reasoning_request(
    routing: &LlmRoutingConfig,
    slot: CapabilitySlot,
    route: &SlotRoute,
    model_spec: &crate::llm::model_catalog::ModelCatalogEntry,
) -> ResolvedReasoningRequest {
    if slot == CapabilitySlot::Vision {
        return ResolvedReasoningRequest::disabled();
    }
    let requested_mode = route.reasoning.map(|r| r.mode).unwrap_or_else(|| {
        if route.thinking {
            ReasoningMode::Auto
        } else {
            ReasoningMode::Off
        }
    });
    if requested_mode == ReasoningMode::Off {
        return ResolvedReasoningRequest::disabled();
    }

    let capability = reasoning_capability_for_route(routing, route, model_spec);
    let effective_mode = resolve_auto_reasoning_mode(slot, requested_mode, &capability);
    let requested = effective_mode != ReasoningMode::Off && capability.can_request();
    ResolvedReasoningRequest {
        mode: effective_mode,
        adapter: capability.adapter,
        control: capability.control,
        visibility: capability.visibility,
        requested,
        isolate_output: capability.needs_isolation()
            || matches!(
                capability.visibility,
                ReasoningVisibility::ContentTag | ReasoningVisibility::PlainContentRisk
            ),
    }
}

fn resolve_auto_reasoning_mode(
    slot: CapabilitySlot,
    mode: ReasoningMode,
    capability: &ReasoningCapability,
) -> ReasoningMode {
    if mode != ReasoningMode::Auto {
        return clamp_reasoning_mode(mode, capability);
    }
    let mode = match capability.control {
        ReasoningControl::None => ReasoningMode::Auto,
        ReasoningControl::Switch => ReasoningMode::Auto,
        ReasoningControl::Tag => ReasoningMode::Auto,
        ReasoningControl::Effort | ReasoningControl::Level | ReasoningControl::Budget => match slot
        {
            CapabilitySlot::Reasoner | CapabilitySlot::LongContext => ReasoningMode::Medium,
            CapabilitySlot::Fast | CapabilitySlot::Writer => ReasoningMode::Low,
            _ => ReasoningMode::Off,
        },
    };
    clamp_reasoning_mode(mode, capability)
}

fn clamp_reasoning_mode(mode: ReasoningMode, capability: &ReasoningCapability) -> ReasoningMode {
    if capability.supported_modes.contains(&mode) {
        return mode;
    }
    if mode == ReasoningMode::Off && capability.disable_supported {
        return ReasoningMode::Off;
    }
    capability.default_mode
}

fn reasoning_capability_for_route(
    routing: &LlmRoutingConfig,
    route: &SlotRoute,
    model_spec: &crate::llm::model_catalog::ModelCatalogEntry,
) -> ReasoningCapability {
    let inferred = infer_reasoning_capability(&route.provider_id, &route.model, model_spec);
    let Some(provider) = routing.providers.get(&route.provider_id) else {
        return inferred;
    };
    let Some(override_row) = provider.model_capabilities.get(&route.model) else {
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
    if provider == "deepseek" && (model.contains("reasoner") || model_spec.supports_thinking) {
        return ReasoningCapability {
            adapter: ReasoningAdapter::DeepSeekReasoningContent,
            control: ReasoningControl::Switch,
            visibility: ReasoningVisibility::HiddenChannel,
            supported_modes: crate::llm::model_catalog::SWITCH_REASONING_MODES.to_vec(),
            default_mode: ReasoningMode::Auto,
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
    if provider == "google" || provider == "gemini" {
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
            default_mode: ReasoningMode::Auto,
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

fn requested_slot(input: CapabilityRouteInput) -> CapabilitySlot {
    if input.has_images || input.intent == AgentIntent::VisionChat {
        return CapabilitySlot::Vision;
    }
    if input.needs_tools || input.intent == AgentIntent::SkillManagement {
        return CapabilitySlot::AgentTools;
    }
    if input.context_tokens > 200_000 {
        return CapabilitySlot::LongContext;
    }
    match input.intent {
        AgentIntent::RewriteSelection
        | AgentIntent::Write
        | AgentIntent::Chapter
        | AgentIntent::DocumentCheck => CapabilitySlot::Writer,
        AgentIntent::Research | AgentIntent::CitationCheck if input.needs_reasoning => {
            CapabilitySlot::Reasoner
        }
        AgentIntent::Research | AgentIntent::CitationCheck => CapabilitySlot::Reasoner,
        AgentIntent::Chat | AgentIntent::AskNotes | AgentIntent::Organize => CapabilitySlot::Fast,
        AgentIntent::VisionChat | AgentIntent::SkillManagement => {
            unreachable!(
                "VisionChat and SkillManagement are handled by early returns in requested_slot()"
            )
        }
    }
}

fn fallback_chain_for(slot: CapabilitySlot) -> Vec<CapabilitySlot> {
    vec![slot]
}

fn slot_display_name(slot: CapabilitySlot) -> &'static str {
    match slot {
        CapabilitySlot::Fast => "Fast",
        CapabilitySlot::Writer => "Writer",
        CapabilitySlot::Reasoner => "Reasoner",
        CapabilitySlot::LongContext => "Long context",
        CapabilitySlot::Vision => "Vision",
        CapabilitySlot::AgentTools => "Agent tools",
        CapabilitySlot::Embedding => "Embedding",
        CapabilitySlot::Reranker => "Reranker",
        CapabilitySlot::LocalPrivate => "Local private",
    }
}

fn slot_key(slot: CapabilitySlot) -> &'static str {
    match slot {
        CapabilitySlot::Fast => "fast",
        CapabilitySlot::Writer => "writer",
        CapabilitySlot::Reasoner => "reasoner",
        CapabilitySlot::LongContext => "long_context",
        CapabilitySlot::Vision => "vision",
        CapabilitySlot::AgentTools => "agent_tools",
        CapabilitySlot::Embedding => "embedding",
        CapabilitySlot::Reranker => "reranker",
        CapabilitySlot::LocalPrivate => "local_private",
    }
}

fn route_satisfies_slot(
    route: &SlotRoute,
    slot: CapabilitySlot,
    privacy_preference: PrivacyPreference,
    registry: &[ModelRegistryEntry],
) -> bool {
    if privacy_preference == PrivacyPreference::LocalOnly {
        return false;
    }
    let catalog_model =
        find_model(&route.model).filter(|model| model.provider_id == route.provider_id);
    let registry_entry = || {
        registry
            .iter()
            .find(|entry| entry.provider_id == route.provider_id && entry.model_id == route.model)
    };
    match slot {
        CapabilitySlot::Vision => {
            if let Some(model) = catalog_model {
                return model.supports_vision;
            }
            registry_entry().is_some_and(|entry| entry.vision_verified_at.is_some())
        }
        CapabilitySlot::AgentTools => {
            let model =
                find_model(&route.model).unwrap_or_else(|| fallback_model(&route.provider_id));
            model.supports_tools
        }
        CapabilitySlot::Fast
        | CapabilitySlot::Writer
        | CapabilitySlot::Reasoner
        | CapabilitySlot::LongContext => {
            catalog_model.is_some()
                || registry_entry().is_some_and(|entry| {
                    entry.text_verified_at.is_some() || entry.vision_verified_at.is_some()
                })
        }
        _ => true,
    }
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

pub fn resolve_for_provider(
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

    let mut resolved = ResolvedLlmConfig {
        provider_id: provider_id.to_string(),
        model: model_id,
        base_url,
        api_key: None,
        thinking: false,
        reasoning: ResolvedReasoningRequest::disabled(),
        input_budget: (model_spec.context_window as f32 * 0.85) as usize,
        output_budget: model_spec.max_output,
        context_strategy: ContextStrategy::Hybrid,
        endpoint_family: model_spec.endpoint_family,
    };
    hydrate_resolved_api_key(db, &mut resolved)?;
    Ok(resolved)
}

impl ResolvedLlmConfig {
    pub fn to_provider_config(&self, scene: AiScene) -> ProviderConfig {
        self.to_provider_config_for_slot(slot_for_legacy_scene(scene))
    }

    pub fn to_provider_config_for_slot(&self, slot: CapabilitySlot) -> ProviderConfig {
        ProviderConfig {
            name: self.provider_id.clone(),
            base_url: self.base_url.clone(),
            api_key: self.api_key.clone(),
            model: self.model.clone(),
            slot,
            endpoint_family: self.endpoint_family,
        }
    }
}

fn slot_for_legacy_scene(scene: AiScene) -> CapabilitySlot {
    match scene {
        AiScene::KnowledgeLookup => CapabilitySlot::Fast,
        AiScene::DraftingAssist => CapabilitySlot::Writer,
        AiScene::ResearchSynthesis => CapabilitySlot::Reasoner,
        _ => CapabilitySlot::Writer,
    }
}

#[cfg(not(test))]
fn hydrate_resolved_api_key(db: &Database, resolved: &mut ResolvedLlmConfig) -> AppResult<()> {
    if crate::llm::providers::requires_api_key(&resolved.provider_id) {
        resolved.api_key = Some(crate::credentials::get_api_key(
            db,
            &credential_service(&resolved.provider_id),
        )?);
    }
    Ok(())
}

#[cfg(test)]
fn hydrate_resolved_api_key(_db: &Database, _resolved: &mut ResolvedLlmConfig) -> AppResult<()> {
    Ok(())
}

pub fn connectivity_status(
    db: &Database,
    active_scene: AiScene,
) -> AppResult<ConnectivityStatusDto> {
    let route_result =
        resolve_capability_route_for_status(db, route_input_for_scene(active_scene))?;
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
                    scene: active_scene.profile().into(),
                    message: err.to_string(),
                },
                search_provider,
                usage_last,
            });
        }
    };
    let llm_configured =
        crate::credentials::api_key_configured(db, &credential_service(&resolved.provider_id))?;
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
        "misconfigured" => "场景未配置有效模型".into(),
        _ => format!("{} / {}", resolved.provider_id, resolved.model),
    };

    Ok(ConnectivityStatusDto {
        llm: LlmConnectivityDto {
            state: llm_state.into(),
            provider_id: resolved.provider_id,
            model: resolved.model,
            scene: active_scene.profile().into(),
            message,
        },
        search_provider,
        usage_last,
    })
}

pub fn save_usage_last(db: &Database, hit: u32, miss: u32) -> AppResult<()> {
    let usage = LlmUsageLast {
        prompt_cache_hit_tokens: hit,
        prompt_cache_miss_tokens: miss,
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    let json = serde_json::to_string(&usage)?;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![USAGE_LAST_KEY, json],
        )?;
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::Database;

    #[test]
    fn default_routing_does_not_write_legacy_scene_routes() {
        let c = deepseek_defaults();
        assert!(c.scenes.is_empty());
        assert!(c.context_strategy.is_empty());
    }

    #[test]
    fn default_routing_has_no_capability_slots() {
        let c = deepseek_defaults();
        assert_eq!(c.schema_version, 4);
        assert!(c.slots.is_empty());
        assert!(c.providers.is_empty());
    }

    #[test]
    fn resolve_requires_user_configured_capability_slot() {
        let db = Database::open_in_memory().expect("mem db");
        let err = resolve_capability_route(
            &db,
            CapabilityRouteInput {
                intent: AgentIntent::AskNotes,
                context_tokens: 0,
                has_images: false,
                needs_tools: false,
                needs_reasoning: false,
                privacy_preference: PrivacyPreference::ExternalAllowed,
            },
        )
        .expect_err("default routing should not select a model");

        assert!(err.to_string().contains("能力槽"));
        assert!(err.to_string().contains("未配置"));
    }

    #[test]
    fn connectivity_status_reports_selected_mcp_search_provider() {
        let db = Database::open_in_memory().expect("mem db");
        let mut routing = deepseek_defaults();
        routing.providers.insert(
            "deepseek".into(),
            ProviderOverride {
                base_url: None,
                label: None,
                default_model: Some("deepseek-v4-flash".into()),
                enabled_models: Some(vec!["deepseek-v4-flash".into()]),
                model_capabilities: std::collections::HashMap::new(),
            },
        );
        routing.slots.insert(
            "fast".into(),
            SlotRoute {
                provider_id: "deepseek".into(),
                model: "deepseek-v4-flash".into(),
                thinking: false,
                reasoning: None,
            },
        );
        save(&db, &routing).expect("save routing");
        crate::credentials::mark_api_key_configured(&db, "iris.llm.deepseek").expect("mark llm");
        crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(
            &db,
            &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
                id: "anysearch".into(),
                name: "AnySearch".into(),
                kind: "mcp".into(),
                enabled: true,
                transport_kind: "stdio".into(),
                transport_config_json: "{}".into(),
                credential_refs_json: "{}".into(),
                web_search_mapping_json: Some(r#"{"tool":"search"}"#.into()),
                web_fetch_mapping_json: None,
            },
        )
        .expect("provider");

        let status = connectivity_status(&db, AiScene::KnowledgeLookup).expect("status");

        assert_eq!(status.llm.state, "ready");
        assert!(status.search_provider.configured);
        assert_eq!(
            status.search_provider.provider_id.as_deref(),
            Some("anysearch")
        );
    }

    #[test]
    fn connectivity_status_accepts_live_validated_custom_text_model() {
        let db = Database::open_in_memory().expect("mem db");
        let mut routing = deepseek_defaults();
        routing.providers.insert(
            "custom".into(),
            ProviderOverride {
                base_url: Some("https://example.com/v1".into()),
                label: Some("Custom".into()),
                default_model: Some("MiniMax-M3".into()),
                enabled_models: Some(vec!["MiniMax-M3".into()]),
                model_capabilities: std::collections::HashMap::new(),
            },
        );
        routing.slots.insert(
            "fast".into(),
            SlotRoute {
                provider_id: "custom".into(),
                model: "MiniMax-M3".into(),
                thinking: false,
                reasoning: None,
            },
        );
        save(&db, &routing).expect("save routing");
        model_registry::mark_model_validated(
            &db,
            "custom",
            "MiniMax-M3",
            model_registry::ModelValidationKind::Text,
        )
        .expect("validate model");
        crate::credentials::mark_api_key_configured(&db, "iris.llm.custom").expect("mark llm");

        let status = connectivity_status(&db, AiScene::KnowledgeLookup).expect("status");

        assert_eq!(status.llm.state, "ready");
        assert_eq!(status.llm.provider_id, "custom");
        assert_eq!(status.llm.model, "MiniMax-M3");
    }

    #[test]
    fn connectivity_status_reports_missing_key_for_validated_custom_model() {
        let db = Database::open_in_memory().expect("mem db");
        let mut routing = deepseek_defaults();
        routing.providers.insert(
            "custom".into(),
            ProviderOverride {
                base_url: Some("https://example.com/v1".into()),
                label: Some("Custom".into()),
                default_model: Some("MiniMax-M3".into()),
                enabled_models: Some(vec!["MiniMax-M3".into()]),
                model_capabilities: std::collections::HashMap::new(),
            },
        );
        routing.slots.insert(
            "fast".into(),
            SlotRoute {
                provider_id: "custom".into(),
                model: "MiniMax-M3".into(),
                thinking: false,
                reasoning: None,
            },
        );
        save(&db, &routing).expect("save routing");
        model_registry::mark_model_validated(
            &db,
            "custom",
            "MiniMax-M3",
            model_registry::ModelValidationKind::Text,
        )
        .expect("validate model");

        let status = connectivity_status(&db, AiScene::KnowledgeLookup).expect("status");

        assert_eq!(status.llm.state, "missing_key");
        assert_eq!(status.llm.provider_id, "custom");
        assert_eq!(status.llm.model, "MiniMax-M3");
    }

    #[test]
    fn connectivity_status_rejects_unvalidated_custom_model() {
        let db = Database::open_in_memory().expect("mem db");
        let mut routing = deepseek_defaults();
        routing.providers.insert(
            "custom".into(),
            ProviderOverride {
                base_url: Some("https://example.com/v1".into()),
                label: Some("Custom".into()),
                default_model: Some("MiniMax-M3".into()),
                enabled_models: Some(vec!["MiniMax-M3".into()]),
                model_capabilities: std::collections::HashMap::new(),
            },
        );
        routing.slots.insert(
            "fast".into(),
            SlotRoute {
                provider_id: "custom".into(),
                model: "MiniMax-M3".into(),
                thinking: false,
                reasoning: None,
            },
        );
        save(&db, &routing).expect("save routing");

        let status = connectivity_status(&db, AiScene::KnowledgeLookup).expect("status");

        assert_eq!(status.llm.state, "misconfigured");
        assert!(status.llm.message.contains("Fast"));
        assert!(status.llm.message.contains("未配置"));
    }

    #[test]
    fn connectivity_status_propagates_invalid_routing_errors() {
        let db = Database::open_in_memory().expect("mem db");
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)",
                rusqlite::params![SETTINGS_KEY, r#"{"version":"bad"}"#],
            )?;
            Ok(())
        })
        .expect("seed invalid routing");

        let err = connectivity_status(&db, AiScene::KnowledgeLookup)
            .expect_err("invalid routing should remain an error");

        assert!(err.to_string().contains("llm_routing"));
    }

    #[test]
    fn load_tolerates_legacy_plain_text_custom_base_url() {
        let db = Database::open_in_memory().expect("mem db");
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)",
                rusqlite::params![LEGACY_CUSTOM_BASE, "https://example.com/v1"],
            )?;
            Ok(())
        })
        .expect("seed");
        let routing = load(&db).expect("load should not fail on legacy plain text");
        assert_eq!(
            routing
                .providers
                .get("custom")
                .and_then(|p| p.base_url.as_deref()),
            Some("https://example.com/v1")
        );
    }

    #[test]
    fn load_invalid_routing_does_not_overwrite_stored_value_with_defaults() {
        let db = Database::open_in_memory().expect("mem db");
        let invalid = serde_json::json!({
            "version": "bad",
            "schemaVersion": 2,
            "providers": {
                "mimo": {
                    "baseUrl": "https://api.xiaomimimo.com/v1",
                    "enabledModels": ["mimo-v2.5"]
                }
            },
            "slots": {},
            "scenes": {},
            "contextStrategy": {}
        })
        .to_string();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)",
                rusqlite::params![SETTINGS_KEY, invalid],
            )?;
            Ok(())
        })
        .expect("seed invalid routing");

        let err = load(&db).expect_err("invalid routing should surface");
        assert!(err.to_string().contains("llm_routing"));

        let stored = db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    [SETTINGS_KEY],
                    |row| row.get::<_, String>(0),
                )
                .map_err(Into::into)
            })
            .expect("stored routing");
        assert!(stored.contains("https://api.xiaomimimo.com/v1"));
        assert!(stored.contains("mimo-v2.5"));
    }

    #[test]
    fn migrate_sets_schema_version_and_created_at() {
        let mut v = serde_json::json!({
            "version": 1,
            "providers": {},
            "scenes": {},
            "contextStrategy": {}
        });
        LlmRoutingConfig::migrate(&mut v).expect("migrate");
        assert_eq!(v["schemaVersion"], serde_json::json!(4));
        assert!(v["createdAt"].is_string());
        assert!(v["slots"].is_object());
    }

    #[test]
    fn migrate_preserves_existing_created_at() {
        let existing = "2024-01-01T00:00:00Z";
        let mut v = serde_json::json!({
            "version": 1,
            "schemaVersion": 0,
            "createdAt": existing,
            "providers": {},
            "scenes": {},
            "contextStrategy": {}
        });
        LlmRoutingConfig::migrate(&mut v).expect("migrate");
        assert_eq!(v["schemaVersion"], serde_json::json!(4));
        assert_eq!(v["createdAt"], serde_json::json!(existing));
    }

    #[test]
    fn migrate_noop_when_already_current() {
        let mut v = serde_json::json!({
            "version": 1,
            "schemaVersion": 4,
            "createdAt": "2024-01-01T00:00:00Z",
            "providers": {},
            "slots": {},
            "scenes": {},
            "contextStrategy": {}
        });
        LlmRoutingConfig::migrate(&mut v).expect("migrate");
        assert_eq!(v["schemaVersion"], serde_json::json!(4));
    }

    #[test]
    fn default_config_has_schema_version() {
        let c = deepseek_defaults();
        assert_eq!(c.schema_version, 4);
        assert!(c.created_at.is_some());
    }

    #[test]
    fn phase3_defaults_leave_capability_slots_unbound() {
        let c = deepseek_defaults();
        assert_eq!(c.schema_version, 4);
        assert!(c.slots.is_empty());
    }

    #[test]
    fn legacy_thinking_true_resolves_to_auto_reasoning() {
        let mut routing = deepseek_defaults();
        routing.providers.insert(
            "deepseek".into(),
            ProviderOverride {
                base_url: None,
                label: None,
                default_model: Some("deepseek-reasoner".into()),
                enabled_models: Some(vec!["deepseek-reasoner".into()]),
                model_capabilities: std::collections::HashMap::new(),
            },
        );
        routing.slots.insert(
            "reasoner".into(),
            SlotRoute {
                provider_id: "deepseek".into(),
                model: "deepseek-reasoner".into(),
                thinking: true,
                reasoning: None,
            },
        );

        let route = resolve_capability_route_from_config(
            &routing,
            CapabilityRouteInput {
                intent: AgentIntent::Research,
                context_tokens: 0,
                has_images: false,
                needs_tools: false,
                needs_reasoning: true,
                privacy_preference: PrivacyPreference::ExternalAllowed,
            },
        )
        .expect("legacy thinking route resolves");

        assert_eq!(route.resolved.reasoning.mode, ReasoningMode::Auto);
        assert_eq!(
            route.resolved.reasoning.adapter,
            ReasoningAdapter::DeepSeekReasoningContent
        );
        assert!(route.resolved.reasoning.requested);
    }

    #[test]
    fn minimax_unknown_reasoning_uses_output_isolation_without_provider_params() {
        let mut routing = deepseek_defaults();
        routing.providers.insert(
            "custom".into(),
            ProviderOverride {
                base_url: Some("https://api.minimax.example/v1".into()),
                label: Some("MiniMax".into()),
                default_model: Some("MiniMax-M3".into()),
                enabled_models: Some(vec!["MiniMax-M3".into()]),
                model_capabilities: std::collections::HashMap::new(),
            },
        );
        routing.slots.insert(
            "fast".into(),
            SlotRoute {
                provider_id: "custom".into(),
                model: "MiniMax-M3".into(),
                thinking: false,
                reasoning: Some(ReasoningSlotConfig {
                    mode: ReasoningMode::Auto,
                }),
            },
        );
        let registry = vec![ModelRegistryEntry {
            provider_id: "custom".into(),
            model_id: "MiniMax-M3".into(),
            display_name: "MiniMax-M3".into(),
            source: model_registry::ModelRegistrySource::Manual,
            stale: false,
            first_seen_at: None,
            last_seen_at: None,
            last_refreshed_at: None,
            text_verified_at: Some("verified".into()),
            vision_verified_at: None,
            user_confirmed_capabilities: Vec::new(),
        }];

        let route = resolve_capability_route_with_registry(
            &routing,
            CapabilityRouteInput {
                intent: AgentIntent::AskNotes,
                context_tokens: 0,
                has_images: false,
                needs_tools: false,
                needs_reasoning: false,
                privacy_preference: PrivacyPreference::ExternalAllowed,
            },
            &registry,
        )
        .expect("validated MiniMax route resolves");

        assert_eq!(
            route.resolved.reasoning.adapter,
            ReasoningAdapter::OpenAiCompatibleTagStream
        );
        assert!(!route.resolved.reasoning.requested);
        assert!(route.resolved.reasoning.isolate_output);
    }

    #[test]
    fn provider_overrides_can_enable_glm_reasoning() {
        let mut routing = deepseek_defaults();
        let mut zhipu_caps = std::collections::HashMap::new();
        zhipu_caps.insert(
            "glm-5-plus".into(),
            ModelCapabilityOverride {
                reasoning_adapter: Some(ReasoningAdapter::GlmThinking),
                reasoning_control: Some(ReasoningControl::Effort),
                reasoning_visibility: Some(ReasoningVisibility::HiddenChannel),
                supported_modes: Some(crate::llm::model_catalog::EFFORT_REASONING_MODES.to_vec()),
                default_mode: Some(ReasoningMode::Medium),
                disable_supported: Some(true),
                user_verified_at: Some("manual".into()),
                probe_verified_at: None,
            },
        );
        routing.providers.insert(
            "zhipu".into(),
            ProviderOverride {
                base_url: None,
                label: None,
                default_model: Some("glm-5-plus".into()),
                enabled_models: Some(vec!["glm-5-plus".into()]),
                model_capabilities: zhipu_caps,
            },
        );
        routing.slots.insert(
            "reasoner".into(),
            SlotRoute {
                provider_id: "zhipu".into(),
                model: "glm-5-plus".into(),
                thinking: false,
                reasoning: Some(ReasoningSlotConfig {
                    mode: ReasoningMode::High,
                }),
            },
        );

        let registry = vec![ModelRegistryEntry {
            provider_id: "zhipu".into(),
            model_id: "glm-5-plus".into(),
            display_name: "glm-5-plus".into(),
            source: model_registry::ModelRegistrySource::Manual,
            stale: false,
            first_seen_at: None,
            last_seen_at: None,
            last_refreshed_at: None,
            text_verified_at: Some("verified".into()),
            vision_verified_at: None,
            user_confirmed_capabilities: Vec::new(),
        }];
        let glm = resolve_capability_route_with_registry(
            &routing,
            CapabilityRouteInput {
                intent: AgentIntent::Research,
                context_tokens: 0,
                has_images: false,
                needs_tools: false,
                needs_reasoning: true,
                privacy_preference: PrivacyPreference::ExternalAllowed,
            },
            &registry,
        )
        .expect("glm route");
        assert_eq!(
            glm.resolved.reasoning.adapter,
            ReasoningAdapter::GlmThinking
        );
        assert_eq!(glm.resolved.reasoning.mode, ReasoningMode::High);
        assert!(glm.resolved.reasoning.requested);
    }

    #[test]
    fn qwen3_uses_chat_template_and_tag_isolation() {
        let mut routing = deepseek_defaults();
        routing.providers.insert(
            "custom".into(),
            ProviderOverride {
                base_url: Some("https://dashscope.example/compatible-mode/v1".into()),
                label: Some("Qwen".into()),
                default_model: Some("qwen3-32b".into()),
                enabled_models: Some(vec!["qwen3-32b".into()]),
                model_capabilities: std::collections::HashMap::new(),
            },
        );
        routing.slots.insert(
            "reasoner".into(),
            SlotRoute {
                provider_id: "custom".into(),
                model: "qwen3-32b".into(),
                thinking: false,
                reasoning: Some(ReasoningSlotConfig {
                    mode: ReasoningMode::Auto,
                }),
            },
        );
        let registry = vec![ModelRegistryEntry {
            provider_id: "custom".into(),
            model_id: "qwen3-32b".into(),
            display_name: "qwen3-32b".into(),
            source: model_registry::ModelRegistrySource::Manual,
            stale: false,
            first_seen_at: None,
            last_seen_at: None,
            last_refreshed_at: None,
            text_verified_at: Some("verified".into()),
            vision_verified_at: None,
            user_confirmed_capabilities: Vec::new(),
        }];

        let qwen = resolve_capability_route_with_registry(
            &routing,
            CapabilityRouteInput {
                intent: AgentIntent::Research,
                context_tokens: 0,
                has_images: false,
                needs_tools: false,
                needs_reasoning: true,
                privacy_preference: PrivacyPreference::ExternalAllowed,
            },
            &registry,
        )
        .expect("qwen route");

        assert_eq!(
            qwen.resolved.reasoning.adapter,
            ReasoningAdapter::QwenChatTemplate
        );
        assert_eq!(
            qwen.resolved.reasoning.visibility,
            ReasoningVisibility::ContentTag
        );
        assert!(qwen.resolved.reasoning.requested);
        assert!(qwen.resolved.reasoning.isolate_output);
    }

    #[test]
    fn vision_slot_ignores_reasoning_configuration() {
        let mut routing = deepseek_defaults();
        routing.providers.insert(
            "openai".into(),
            ProviderOverride {
                base_url: None,
                label: None,
                default_model: Some("gpt-4o".into()),
                enabled_models: Some(vec!["gpt-4o".into()]),
                model_capabilities: std::collections::HashMap::new(),
            },
        );
        routing.slots.insert(
            "vision".into(),
            SlotRoute {
                provider_id: "openai".into(),
                model: "gpt-4o".into(),
                thinking: true,
                reasoning: Some(ReasoningSlotConfig {
                    mode: ReasoningMode::High,
                }),
            },
        );

        let route = resolve_capability_route_from_config(
            &routing,
            CapabilityRouteInput {
                intent: AgentIntent::VisionChat,
                context_tokens: 0,
                has_images: true,
                needs_tools: false,
                needs_reasoning: true,
                privacy_preference: PrivacyPreference::ExternalAllowed,
            },
        )
        .expect("vision route");

        assert_eq!(
            route.resolved.reasoning,
            ResolvedReasoningRequest::disabled()
        );
        assert!(!route.resolved.thinking);
    }

    #[test]
    fn sanitize_removes_legacy_ollama_routes() {
        let mut routing = deepseek_defaults();
        routing.slots.insert(
            "fast".into(),
            SlotRoute {
                provider_id: "ollama".into(),
                model: "llama3.2".into(),
                thinking: false,
                reasoning: None,
            },
        );
        routing.providers.insert(
            "ollama".into(),
            ProviderOverride {
                base_url: Some("http://127.0.0.1:11434".into()),
                label: Some("Ollama".into()),
                default_model: Some("llama3.2".into()),
                enabled_models: Some(vec!["llama3.2".into()]),
                model_capabilities: std::collections::HashMap::new(),
            },
        );

        sanitize_routing(&mut routing);

        assert!(!routing.slots.contains_key("fast"));
        assert!(!routing.providers.contains_key("ollama"));
    }

    #[test]
    fn migrate_legacy_scenes_to_phase3_slots() {
        let mut v = serde_json::json!({
            "version": 1,
            "schemaVersion": 1,
            "createdAt": "2024-01-01T00:00:00Z",
            "providers": {
                "openai": { "baseUrl": "https://api.openai.com/v1" }
            },
            "scenes": {
                "knowledge_lookup": {
                    "providerId": "openai",
                    "model": "gpt-4o-mini",
                    "thinking": false
                },
                "drafting_assist": {
                    "providerId": "anthropic",
                    "model": "claude-3-5-haiku-20241022",
                    "thinking": false
                }
            },
            "contextStrategy": {}
        });

        LlmRoutingConfig::migrate(&mut v).expect("migrate");

        assert_eq!(v["schemaVersion"], serde_json::json!(4));
        assert_eq!(v["slots"]["fast"]["providerId"], "openai");
        assert_eq!(v["slots"]["fast"]["model"], "gpt-4o-mini");
        assert_eq!(v["slots"]["fast"]["reasoning"]["mode"], "off");
        assert_eq!(v["slots"]["writer"]["providerId"], "anthropic");
        assert!(v["slots"].get("agent_tools").is_none());
    }

    #[test]
    fn load_treats_null_model_capabilities_as_empty_map() {
        let db = Database::open_in_memory().expect("mem db");
        let dirty = serde_json::json!({
            "version": 1,
            "schemaVersion": 4,
            "createdAt": "2026-07-05T00:00:00Z",
            "providers": {
                "custom": {
                    "baseUrl": "https://api.example.com/v1",
                    "label": "Custom",
                    "defaultModel": "model-a",
                    "enabledModels": ["model-a"],
                    "modelCapabilities": null
                },
                "custom_snake": {
                    "baseUrl": "https://api.example.com/v1",
                    "label": "Custom Snake",
                    "defaultModel": "model-b",
                    "enabledModels": ["model-b"],
                    "model_capabilities": null
                }
            },
            "slots": {},
            "scenes": {},
            "contextStrategy": {}
        })
        .to_string();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)",
                rusqlite::params![SETTINGS_KEY, dirty],
            )?;
            Ok(())
        })
        .expect("seed dirty routing");

        let routing = load(&db).expect("null model capabilities should be normalized");

        assert!(routing.providers["custom"].model_capabilities.is_empty());
        assert!(routing.providers["custom_snake"]
            .model_capabilities
            .is_empty());
    }

    #[test]
    fn load_treats_null_routing_maps_as_empty_maps() {
        let db = Database::open_in_memory().expect("mem db");
        let dirty = serde_json::json!({
            "version": 1,
            "schemaVersion": 4,
            "createdAt": "2026-07-05T00:00:00Z",
            "providers": null,
            "slots": null,
            "scenes": null,
            "contextStrategy": null
        })
        .to_string();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)",
                rusqlite::params![SETTINGS_KEY, dirty],
            )?;
            Ok(())
        })
        .expect("seed dirty routing");

        let routing = load(&db).expect("null routing maps should be normalized");

        assert!(routing.providers.is_empty());
        assert!(routing.slots.is_empty());
        assert!(routing.scenes.is_empty());
        assert!(routing.context_strategy.is_empty());
    }

    #[test]
    fn load_rewrites_dirty_v3_routing_to_clean_v4() {
        let db = Database::open_in_memory().expect("mem db");
        let dirty = serde_json::json!({
            "version": 1,
            "schemaVersion": 3,
            "createdAt": "2026-07-05T00:00:00Z",
            "providers": {
                "custom": {
                    "baseUrl": "https://api.example.com/v1",
                    "label": "Custom",
                    "defaultModel": "model-a",
                    "enabledModels": ["model-a"],
                    "modelCapabilities": null
                }
            },
            "slots": {
                "fast": {
                    "providerId": "custom",
                    "model": "model-a",
                    "thinking": true
                }
            },
            "scenes": null,
            "contextStrategy": null
        })
        .to_string();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)",
                rusqlite::params![SETTINGS_KEY, dirty],
            )?;
            Ok(())
        })
        .expect("seed dirty routing");

        let routing = load(&db).expect("dirty v3 routing should load");

        assert_eq!(routing.schema_version, 4);
        assert_eq!(
            routing.slots["fast"].reasoning.map(|value| value.mode),
            Some(ReasoningMode::Auto)
        );

        let stored = db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    [SETTINGS_KEY],
                    |row| row.get::<_, String>(0),
                )
                .map_err(Into::into)
            })
            .expect("stored routing");
        let value: serde_json::Value = serde_json::from_str(&stored).expect("stored json");
        assert_eq!(value["schemaVersion"], serde_json::json!(4));
        assert!(value["providers"]["custom"]
            .get("modelCapabilities")
            .is_none());
        assert!(value
            .get("contextStrategy")
            .is_none_or(serde_json::Value::is_object));
        assert!(value["slots"]["fast"]["reasoning"].is_object());
    }

    #[test]
    fn load_rejects_non_object_routing_with_clear_shape_error() {
        let db = Database::open_in_memory().expect("mem db");
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)",
                rusqlite::params![SETTINGS_KEY, "null"],
            )?;
            Ok(())
        })
        .expect("seed non-object routing");

        let err = load(&db).expect_err("non-object routing should remain invalid");

        assert!(err.to_string().contains("llm_routing"));
        assert!(err.to_string().contains("object"));
    }

    #[test]
    fn load_does_not_rewrite_current_routing_only_missing_optional_maps() {
        let db = Database::open_in_memory().expect("mem db");
        let current = serde_json::json!({
            "version": 1,
            "schemaVersion": 4,
            "createdAt": "2026-07-05T00:00:00Z",
            "providers": {},
            "slots": {}
        })
        .to_string();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)",
                rusqlite::params![SETTINGS_KEY, current],
            )?;
            Ok(())
        })
        .expect("seed current routing");

        let _routing = load(&db).expect("current routing should load");

        let stored = db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    [SETTINGS_KEY],
                    |row| row.get::<_, String>(0),
                )
                .map_err(Into::into)
            })
            .expect("stored routing");
        let value: serde_json::Value = serde_json::from_str(&stored).expect("stored json");
        assert!(value.get("scenes").is_none());
        assert!(value.get("contextStrategy").is_none());
    }

    #[test]
    fn migrate_cleans_legacy_default_slot_bindings() {
        let mut v = serde_json::json!({
            "version": 1,
            "schemaVersion": 2,
            "createdAt": "2024-01-01T00:00:00Z",
            "providers": {},
            "slots": {
                "fast": {
                    "providerId": "deepseek",
                    "model": "deepseek-v4-flash",
                    "thinking": false
                },
                "vision": {
                    "providerId": "mimo",
                    "model": "mimo-v2.5",
                    "thinking": false
                }
            },
            "contextStrategy": {}
        });

        LlmRoutingConfig::migrate(&mut v).expect("migrate");

        assert_eq!(v["schemaVersion"], serde_json::json!(4));
        assert_eq!(v["slots"], serde_json::json!({}));
    }

    #[test]
    fn sanitize_preserves_phase3_builtin_providers() {
        let mut c = deepseek_defaults();
        c.slots.insert(
            "vision".into(),
            SlotRoute {
                provider_id: "openai".into(),
                model: "gpt-4o".into(),
                thinking: false,
                reasoning: None,
            },
        );
        c.providers.insert(
            "openai".into(),
            ProviderOverride {
                base_url: None,
                label: None,
                default_model: Some("gpt-4o".into()),
                enabled_models: Some(vec!["gpt-4o".into()]),
                model_capabilities: std::collections::HashMap::new(),
            },
        );
        c.providers.insert(
            "anthropic".into(),
            ProviderOverride {
                base_url: Some("https://api.anthropic.com".into()),
                label: None,
                default_model: Some("claude-3-5-haiku-20241022".into()),
                enabled_models: Some(vec!["claude-3-5-haiku-20241022".into()]),
                model_capabilities: std::collections::HashMap::new(),
            },
        );

        sanitize_routing(&mut c);

        assert_eq!(
            c.slots.get("vision").map(|r| r.provider_id.as_str()),
            Some("openai")
        );
        assert!(c.providers.contains_key("anthropic"));
    }

    #[test]
    fn route_input_requires_available_vision_slot_without_fast_fallback() {
        let routing = deepseek_defaults();
        let err = resolve_capability_route_from_config(
            &routing,
            CapabilityRouteInput {
                intent: crate::ai_types::AgentIntent::VisionChat,
                context_tokens: 1_000,
                has_images: true,
                needs_tools: false,
                needs_reasoning: false,
                privacy_preference: PrivacyPreference::ExternalAllowed,
            },
        )
        .expect_err("vision route should not fall back to Fast");

        assert!(err.to_string().contains("Vision"));
        assert!(err.to_string().contains("未配置"));
        assert!(!err.to_string().contains("Fast"));
    }

    #[test]
    fn custom_text_validated_model_can_route_reasoner_and_long_context_slots() {
        let mut routing = deepseek_defaults();
        routing.providers.insert(
            "custom".into(),
            ProviderOverride {
                base_url: Some("https://example.com/v1".into()),
                label: Some("Custom".into()),
                default_model: Some("plain-text".into()),
                enabled_models: Some(vec!["plain-text".into()]),
                model_capabilities: std::collections::HashMap::new(),
            },
        );
        for slot in ["reasoner", "long_context"] {
            routing.slots.insert(
                slot.into(),
                SlotRoute {
                    provider_id: "custom".into(),
                    model: "plain-text".into(),
                    thinking: false,
                    reasoning: None,
                },
            );
        }
        let registry = vec![ModelRegistryEntry {
            provider_id: "custom".into(),
            model_id: "plain-text".into(),
            display_name: "plain-text".into(),
            source: model_registry::ModelRegistrySource::Manual,
            stale: false,
            first_seen_at: None,
            last_seen_at: None,
            last_refreshed_at: None,
            text_verified_at: Some("verified".into()),
            vision_verified_at: None,
            user_confirmed_capabilities: Vec::new(),
        }];

        let reasoner = resolve_capability_route_with_registry(
            &routing,
            CapabilityRouteInput {
                intent: AgentIntent::Research,
                context_tokens: 1_000,
                has_images: false,
                needs_tools: false,
                needs_reasoning: true,
                privacy_preference: PrivacyPreference::ExternalAllowed,
            },
            &registry,
        )
        .expect("text-validated model should route reasoner");
        assert_eq!(reasoner.summary.slot, CapabilitySlot::Reasoner);
        assert_eq!(reasoner.summary.model, "plain-text");

        let long_context = resolve_capability_route_with_registry(
            &routing,
            CapabilityRouteInput {
                intent: AgentIntent::AskNotes,
                context_tokens: 240_000,
                has_images: false,
                needs_tools: false,
                needs_reasoning: false,
                privacy_preference: PrivacyPreference::ExternalAllowed,
            },
            &registry,
        )
        .expect("text-validated model should route long context");
        assert_eq!(long_context.summary.slot, CapabilitySlot::LongContext);
        assert_eq!(long_context.summary.model, "plain-text");
    }
    #[test]
    fn vision_route_ignores_dirty_verified_state_for_catalog_non_vision_model() {
        let mut routing = deepseek_defaults();
        routing.slots.insert(
            "vision".into(),
            SlotRoute {
                provider_id: "deepseek".into(),
                model: "deepseek-v4-flash".into(),
                thinking: false,
                reasoning: None,
            },
        );
        let registry = vec![ModelRegistryEntry {
            provider_id: "deepseek".into(),
            model_id: "deepseek-v4-flash".into(),
            display_name: "DeepSeek V4 Flash".into(),
            source: model_registry::ModelRegistrySource::Manual,
            stale: false,
            first_seen_at: None,
            last_seen_at: None,
            last_refreshed_at: None,
            text_verified_at: None,
            vision_verified_at: Some("dirty".into()),
            user_confirmed_capabilities: Vec::new(),
        }];

        let err = resolve_capability_route_with_registry(
            &routing,
            CapabilityRouteInput {
                intent: crate::ai_types::AgentIntent::VisionChat,
                context_tokens: 1_000,
                has_images: true,
                needs_tools: false,
                needs_reasoning: false,
                privacy_preference: PrivacyPreference::ExternalAllowed,
            },
            &registry,
        )
        .expect_err("dirty DeepSeek vision verification must not make it routable");

        assert!(err.to_string().contains("Vision"));
        assert!(err.to_string().contains("未配置"));
    }

    #[test]
    fn builtin_mimo_uses_managed_default_base_url() {
        let db = Database::open_in_memory().expect("mem db");
        let mut routing = deepseek_defaults();
        routing.providers.insert(
            "mimo".into(),
            ProviderOverride {
                base_url: None,
                label: None,
                default_model: Some("mimo-v2.5".into()),
                enabled_models: Some(vec!["mimo-v2.5".into()]),
                model_capabilities: std::collections::HashMap::new(),
            },
        );
        save(&db, &routing).expect("save routing");

        let resolved = resolve_for_provider(&db, "mimo", None).expect("resolve MiMo");

        assert_eq!(resolved.provider_id, "mimo");
        assert_eq!(resolved.base_url, "https://api.xiaomimimo.com/v1");
    }

    #[test]
    fn custom_provider_without_base_url_is_rejected() {
        let db = Database::open_in_memory().expect("mem db");
        let mut routing = deepseek_defaults();
        routing
            .providers
            .insert("custom".into(), ProviderOverride::default());
        save(&db, &routing).expect("save routing");

        let err = resolve_for_provider(&db, "custom", None).expect_err("missing custom base URL");

        assert!(err.to_string().contains("Custom"));
        assert!(err.to_string().contains("Base URL"));
    }

    #[test]
    fn local_private_is_not_bound_to_external_deepseek_by_default() {
        let routing = deepseek_defaults();

        assert!(!routing.slots.contains_key("local_private"));
    }

    #[test]
    fn sanitize_migrates_legacy_mimo_experimental_model() {
        let mut routing = deepseek_defaults();
        routing.slots.insert(
            "vision".into(),
            SlotRoute {
                provider_id: "mimo".into(),
                model: "mimo-vl-7b-experimental".into(),
                thinking: false,
                reasoning: None,
            },
        );
        routing.providers.insert(
            "mimo".into(),
            ProviderOverride {
                base_url: None,
                label: None,
                default_model: Some("mimo-vl-7b-experimental".into()),
                enabled_models: Some(vec!["mimo-vl-7b-experimental".into()]),
                model_capabilities: std::collections::HashMap::new(),
            },
        );

        sanitize_routing(&mut routing);

        assert_eq!(
            routing
                .slots
                .get("vision")
                .map(|route| route.model.as_str()),
            Some("MiMo-V2.5-Pro")
        );
    }
}
