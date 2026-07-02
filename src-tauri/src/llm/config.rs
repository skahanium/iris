//! Unified LLM routing configuration (`settings.llm_routing`).

use serde::{Deserialize, Serialize};

use crate::ai_types::{
    AgentIntent, AiScene, CapabilityRouteSummary, CapabilitySlot, EndpointFamily, ProviderConfig,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneRoute {
    #[serde(alias = "provider_id")]
    pub provider_id: String,
    pub model: String,
    #[serde(default)]
    pub thinking: bool,
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
    const CURRENT_SCHEMA_VERSION: u32 = 2;

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

        Ok(())
    }
}

fn slots_from_legacy_scenes(
    config: &serde_json::Value,
) -> std::collections::HashMap<String, serde_json::Value> {
    let defaults = deepseek_defaults();
    let mut slots: std::collections::HashMap<String, serde_json::Value> = defaults
        .slots
        .into_iter()
        .map(|(slot, route)| (slot, serde_json::to_value(route).unwrap_or_default()))
        .collect();
    let scenes = &config["scenes"];
    let mappings = [
        ("fast", "knowledge_lookup"),
        ("writer", "drafting_assist"),
        ("reasoner", "research_synthesis"),
        ("long_context", "exemplar_learning"),
        ("agent_tools", "knowledge_lookup"),
    ];
    for (slot, scene) in mappings {
        if scenes[scene].is_object() {
            slots.insert(slot.to_string(), scenes[scene].clone());
        }
    }
    slots
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
        ("agent_tools", "knowledge_lookup"),
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
    let mut slots = std::collections::HashMap::new();
    slots.insert(
        "fast".into(),
        SlotRoute {
            provider_id: "deepseek".into(),
            model: "deepseek-v4-flash".into(),
            thinking: false,
        },
    );
    slots.insert(
        "writer".into(),
        SlotRoute {
            provider_id: "deepseek".into(),
            model: "deepseek-v4-pro".into(),
            thinking: false,
        },
    );
    slots.insert(
        "reasoner".into(),
        SlotRoute {
            provider_id: "deepseek".into(),
            model: "deepseek-v4-pro".into(),
            thinking: true,
        },
    );
    slots.insert(
        "long_context".into(),
        SlotRoute {
            provider_id: "deepseek".into(),
            model: "deepseek-v4-pro".into(),
            thinking: false,
        },
    );
    slots.insert(
        "agent_tools".into(),
        SlotRoute {
            provider_id: "deepseek".into(),
            model: "deepseek-v4-pro".into(),
            thinking: true,
        },
    );
    slots.insert(
        "vision".into(),
        SlotRoute {
            provider_id: "mimo".into(),
            model: "mimo-v2.5".into(),
            thinking: false,
        },
    );
    slots.insert(
        "embedding".into(),
        SlotRoute {
            provider_id: "deepseek".into(),
            model: "deepseek-v4-flash".into(),
            thinking: false,
        },
    );
    slots.insert(
        "reranker".into(),
        SlotRoute {
            provider_id: "deepseek".into(),
            model: "deepseek-v4-flash".into(),
            thinking: false,
        },
    );
    LlmRoutingConfig {
        version: 1,
        schema_version: LlmRoutingConfig::CURRENT_SCHEMA_VERSION,
        created_at: Some(chrono::Utc::now().to_rfc3339()),
        updated_at: None,
        providers: std::collections::HashMap::new(),
        slots,
        scenes: std::collections::HashMap::new(),
        context_strategy: std::collections::HashMap::new(),
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
            LlmRoutingConfig::migrate(&mut value)
                .map_err(|e| AppError::msg(format!("invalid {SETTINGS_KEY} migration: {e}")))?;
            serde_json::from_value(value)
                .map_err(|e| AppError::msg(format!("invalid {SETTINGS_KEY}: {e}")))?
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
    for route in config.slots.values_mut() {
        if !crate::llm::providers::is_allowed_provider(&route.provider_id) {
            route.provider_id = "deepseek".into();
            route.model = "deepseek-v4-flash".into();
            route.thinking = false;
        } else {
            route.model = normalize_legacy_model_id(&route.model).into();
        }
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
    if config
        .slots
        .get("local_private")
        .is_some_and(|route| route.provider_id == "deepseek")
    {
        config.slots.remove("local_private");
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
    let defaults = deepseek_defaults();
    for (key, route) in defaults.slots {
        config.slots.entry(key).or_insert(route);
    }
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

pub fn resolve_capability_route_from_config(
    routing: &LlmRoutingConfig,
    input: CapabilityRouteInput,
) -> AppResult<ResolvedCapabilityRoute> {
    resolve_capability_route_with_registry(routing, input, &[])
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

    Err(last_error.unwrap_or_else(|| AppError::msg("no capability route available")))
}

fn resolve_route(
    routing: &LlmRoutingConfig,
    slot: CapabilitySlot,
    route: SlotRoute,
) -> AppResult<ResolvedLlmConfig> {
    let provider_override = routing.providers.get(&route.provider_id);
    let custom_base = provider_override.and_then(configured_base_url);
    if requires_base_url(&route.provider_id) && custom_base.is_none() {
        return Err(AppError::msg(format!(
            "{} 需配置 Base URL 后才能调用",
            provider_label(&route.provider_id)
        )));
    }
    let base_url = api_base(&route.provider_id, custom_base);
    let model_spec = find_model(&route.model).unwrap_or_else(|| fallback_model(&route.provider_id));
    let thinking =
        route.thinking || model_spec.supports_thinking && route.model.contains("reasoner");
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
        input_budget,
        output_budget,
        context_strategy,
        endpoint_family: model_spec.endpoint_family,
    })
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
    match slot {
        CapabilitySlot::Vision => vec![CapabilitySlot::Vision],
        CapabilitySlot::AgentTools => vec![
            CapabilitySlot::AgentTools,
            CapabilitySlot::Reasoner,
            CapabilitySlot::Fast,
        ],
        CapabilitySlot::LongContext => vec![
            CapabilitySlot::LongContext,
            CapabilitySlot::Reasoner,
            CapabilitySlot::Fast,
        ],
        CapabilitySlot::Writer => vec![CapabilitySlot::Writer, CapabilitySlot::Fast],
        CapabilitySlot::Reasoner => vec![CapabilitySlot::Reasoner, CapabilitySlot::Fast],
        other => vec![other],
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
    match slot {
        CapabilitySlot::Vision => {
            if let Some(model) =
                find_model(&route.model).filter(|model| model.provider_id == route.provider_id)
            {
                return model.supports_vision;
            }
            registry.iter().any(|entry| {
                entry.provider_id == route.provider_id
                    && entry.model_id == route.model
                    && entry.vision_verified_at.is_some()
            })
        }
        CapabilitySlot::AgentTools => {
            let model =
                find_model(&route.model).unwrap_or_else(|| fallback_model(&route.provider_id));
            model.supports_tools
        }
        CapabilitySlot::Reasoner => {
            let model =
                find_model(&route.model).unwrap_or_else(|| fallback_model(&route.provider_id));
            model.supports_thinking || model.supports_tools
        }
        CapabilitySlot::LongContext => {
            let model =
                find_model(&route.model).unwrap_or_else(|| fallback_model(&route.provider_id));
            model.context_window >= 128_000
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
        other => other,
    }
}

pub fn resolve_for_provider(
    db: &Database,
    provider_id: &str,
    model: Option<&str>,
) -> AppResult<ResolvedLlmConfig> {
    let routing = load(db)?;
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
    let routing = load(db)?;
    let resolved =
        resolve_capability_route_from_config(&routing, route_input_for_scene(active_scene))?
            .resolved;
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

    let selected_web_provider =
        crate::ai_runtime::mcp_runtime_registry::resolve_selected_web_search_provider(db).ok();
    let search_provider = SearchProviderConnectivityDto {
        configured: selected_web_provider.is_some(),
        provider_id: selected_web_provider.map(|provider| provider.id),
    };

    let usage_last = read_usage_last(db)?;

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
    fn resolve_uses_deepseek_flash_for_knowledge() {
        let db = Database::open_in_memory().expect("mem db");
        let resolved = resolve_capability_route(
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
        .expect("resolve")
        .resolved;
        assert_eq!(resolved.provider_id, "deepseek");
        assert_eq!(resolved.model, "deepseek-v4-flash");
    }

    #[test]
    fn connectivity_status_reports_selected_mcp_search_provider() {
        let db = Database::open_in_memory().expect("mem db");
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
        assert_eq!(v["schemaVersion"], serde_json::json!(2));
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
        assert_eq!(v["schemaVersion"], serde_json::json!(2));
        assert_eq!(v["createdAt"], serde_json::json!(existing));
    }

    #[test]
    fn migrate_noop_when_already_current() {
        let mut v = serde_json::json!({
            "version": 1,
            "schemaVersion": 2,
            "createdAt": "2024-01-01T00:00:00Z",
            "providers": {},
            "slots": {},
            "scenes": {},
            "contextStrategy": {}
        });
        LlmRoutingConfig::migrate(&mut v).expect("migrate");
        assert_eq!(v["schemaVersion"], serde_json::json!(2));
    }

    #[test]
    fn default_config_has_schema_version() {
        let c = deepseek_defaults();
        assert_eq!(c.schema_version, 2);
        assert!(c.created_at.is_some());
    }

    #[test]
    fn phase3_defaults_route_by_capability_slots() {
        let c = deepseek_defaults();
        assert_eq!(c.schema_version, 2);
        assert_eq!(
            c.slots.get("fast").map(|r| r.model.as_str()),
            Some("deepseek-v4-flash")
        );
        assert_eq!(
            c.slots.get("writer").map(|r| r.model.as_str()),
            Some("deepseek-v4-pro")
        );
        assert_eq!(
            c.slots.get("agent_tools").map(|r| r.provider_id.as_str()),
            Some("deepseek")
        );
        assert!(c
            .slots
            .values()
            .all(|route| route.provider_id.as_str() != "ollama"));
    }

    #[test]
    fn sanitize_rewrites_legacy_ollama_routes_to_deepseek() {
        let mut routing = deepseek_defaults();
        routing.slots.insert(
            "fast".into(),
            SlotRoute {
                provider_id: "ollama".into(),
                model: "llama3.2".into(),
                thinking: false,
            },
        );
        routing.providers.insert(
            "ollama".into(),
            ProviderOverride {
                base_url: Some("http://127.0.0.1:11434".into()),
                label: Some("Ollama".into()),
                default_model: Some("llama3.2".into()),
                enabled_models: Some(vec!["llama3.2".into()]),
            },
        );

        sanitize_routing(&mut routing);

        let fast = routing.slots.get("fast").expect("fast slot");
        assert_eq!(fast.provider_id, "deepseek");
        assert_eq!(fast.model, "deepseek-v4-flash");
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

        assert_eq!(v["schemaVersion"], serde_json::json!(2));
        assert_eq!(v["slots"]["fast"]["providerId"], "openai");
        assert_eq!(v["slots"]["fast"]["model"], "gpt-4o-mini");
        assert_eq!(v["slots"]["writer"]["providerId"], "anthropic");
        assert_eq!(v["slots"]["agent_tools"]["providerId"], "openai");
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
            },
        );
        c.providers.insert(
            "anthropic".into(),
            ProviderOverride {
                base_url: Some("https://api.anthropic.com".into()),
                label: None,
                default_model: Some("claude-3-5-haiku-20241022".into()),
                enabled_models: Some(vec!["claude-3-5-haiku-20241022".into()]),
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

        assert!(err.to_string().contains("MiMo"));
        assert!(!err.to_string().contains("Fast"));
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

        assert!(err.to_string().contains("no capability route available"));
    }

    #[test]
    fn mimo_without_base_url_is_not_resolved_to_openai() {
        let db = Database::open_in_memory().expect("mem db");
        let err = resolve_for_provider(&db, "mimo", None).expect_err("missing MiMo base URL");

        assert!(err.to_string().contains("MiMo"));
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
