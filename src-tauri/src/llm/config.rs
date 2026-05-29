//! Unified LLM routing configuration (`settings.llm_routing`).

use serde::{Deserialize, Serialize};

use crate::ai_runtime::model_gateway::{ModelGateway, ProviderConfig};
use crate::ai_runtime::scene_router::resolve_scene;
use crate::ai_runtime::AiScene;
use crate::credentials;
use crate::error::{AppError, AppResult};
use crate::llm::model_catalog::{fallback_model, find_model};
use crate::llm::providers::{api_base, credential_service};
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
    pub scenes: std::collections::HashMap<String, SceneRoute>,
    #[serde(default)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextStrategy {
    Hybrid,
    LongContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedLlmConfig {
    pub provider_id: String,
    pub model: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub thinking: bool,
    pub input_budget: usize,
    pub output_budget: u32,
    pub context_strategy: ContextStrategy,
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
    pub search_api: SearchApiConnectivityDto,
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
pub struct SearchApiConnectivityDto {
    pub minimax_configured: bool,
    pub effective_backend: String,
}

impl Default for LlmRoutingConfig {
    fn default() -> Self {
        deepseek_defaults()
    }
}

impl LlmRoutingConfig {
    /// 当前 schema 版本
    const CURRENT_SCHEMA_VERSION: u32 = 1;

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

        Ok(())
    }
}

/// Factory defaults aligned with user preference (DeepSeek V4 Flash / Pro).
pub fn deepseek_defaults() -> LlmRoutingConfig {
    let mut scenes = std::collections::HashMap::new();
    scenes.insert(
        "knowledge_lookup".into(),
        SceneRoute {
            provider_id: "deepseek".into(),
            model: "deepseek-v4-flash".into(),
            thinking: false,
        },
    );
    scenes.insert(
        "exemplar_learning".into(),
        SceneRoute {
            provider_id: "deepseek".into(),
            model: "deepseek-v4-flash".into(),
            thinking: false,
        },
    );
    scenes.insert(
        "drafting_assist".into(),
        SceneRoute {
            provider_id: "deepseek".into(),
            model: "deepseek-v4-pro".into(),
            thinking: false,
        },
    );
    scenes.insert(
        "research_synthesis".into(),
        SceneRoute {
            provider_id: "deepseek".into(),
            model: "deepseek-v4-pro".into(),
            thinking: false,
        },
    );

    let mut context_strategy = std::collections::HashMap::new();
    context_strategy.insert("knowledge_lookup".into(), ContextStrategy::Hybrid);
    context_strategy.insert("exemplar_learning".into(), ContextStrategy::LongContext);
    context_strategy.insert("drafting_assist".into(), ContextStrategy::LongContext);
    context_strategy.insert("research_synthesis".into(), ContextStrategy::Hybrid);

    LlmRoutingConfig {
        version: 1,
        schema_version: LlmRoutingConfig::CURRENT_SCHEMA_VERSION,
        created_at: Some(chrono::Utc::now().to_rfc3339()),
        updated_at: None,
        providers: std::collections::HashMap::new(),
        scenes,
        context_strategy,
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
            let mut value: serde_json::Value = serde_json::from_str(&json)?;
            LlmRoutingConfig::migrate(&mut value)?;
            match serde_json::from_value(value) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("invalid {SETTINGS_KEY}, resetting to defaults: {e}");
                    let defaults = migrate_legacy(db);
                    save(db, &defaults)?;
                    defaults
                }
            }
        }
        None => {
            let migrated = migrate_legacy(db);
            save(db, &migrated)?;
            migrated
        }
    };

    sanitize_routing(&mut config);
    ensure_scene_keys(&mut config);
    Ok(config)
}

/// 移除已下线厂商，并将场景路由迁到 DeepSeek / 自定义端点。
fn sanitize_routing(config: &mut LlmRoutingConfig) {
    const LEGACY: &[&str] = &["openai", "anthropic", "ollama"];
    for route in config.scenes.values_mut() {
        if LEGACY.contains(&route.provider_id.as_str())
            || !crate::llm::providers::is_allowed_provider(&route.provider_id)
        {
            route.provider_id = "deepseek".into();
            route.model = "deepseek-v4-flash".into();
            route.thinking = false;
        }
    }
    config
        .providers
        .retain(|id, _| *id == "deepseek" || crate::llm::providers::is_custom_provider(id));
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

fn ensure_scene_keys(config: &mut LlmRoutingConfig) {
    let defaults = deepseek_defaults();
    for (key, route) in defaults.scenes {
        config.scenes.entry(key).or_insert(route);
    }
    for (key, strategy) in defaults.context_strategy {
        config.context_strategy.entry(key).or_insert(strategy);
    }
}

pub fn resolve_for_scene(db: &Database, scene: AiScene) -> AppResult<ResolvedLlmConfig> {
    let routing = load(db)?;
    let scene_key = scene.profile();
    let route = routing
        .scenes
        .get(scene_key)
        .cloned()
        .ok_or_else(|| AppError::msg(format!("no route for scene {scene_key}")))?;

    let provider_override = routing.providers.get(&route.provider_id);
    let custom_base = provider_override.and_then(|p| p.base_url.as_deref());
    let base_url = api_base(&route.provider_id, custom_base);

    let api_key = credentials::get_secret(&credential_service(&route.provider_id)).ok();

    let model_spec = find_model(&route.model).unwrap_or_else(|| fallback_model(&route.provider_id));
    let thinking =
        route.thinking || model_spec.supports_thinking && route.model.contains("reasoner");

    let profile = resolve_scene(scene);
    let output_budget = model_spec.max_output.min(profile.max_token_budget as u32);

    let input_ratio = if model_spec.context_window >= 256_000 {
        0.85_f32
    } else {
        0.5_f32
    };
    let input_budget = ((model_spec.context_window as f32) * input_ratio) as usize;
    let input_budget = input_budget.saturating_sub(output_budget as usize);

    let context_strategy = routing
        .context_strategy
        .get(scene_key)
        .copied()
        .unwrap_or_else(|| default_strategy_for_scene(scene));

    Ok(ResolvedLlmConfig {
        provider_id: route.provider_id,
        model: route.model,
        base_url,
        api_key,
        thinking,
        input_budget,
        output_budget,
        context_strategy,
    })
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
    let custom_base = provider_override.and_then(|p| p.base_url.as_deref());
    let base_url = api_base(provider_id, custom_base);
    let api_key = credentials::get_secret(&credential_service(provider_id)).ok();
    let model_spec = find_model(&model_id).unwrap_or_else(|| fallback_model(provider_id));

    Ok(ResolvedLlmConfig {
        provider_id: provider_id.to_string(),
        model: model_id,
        base_url,
        api_key,
        thinking: false,
        input_budget: (model_spec.context_window as f32 * 0.85) as usize,
        output_budget: model_spec.max_output,
        context_strategy: ContextStrategy::Hybrid,
    })
}

impl ResolvedLlmConfig {
    pub fn to_provider_config(&self, scene: AiScene) -> ProviderConfig {
        ProviderConfig {
            name: self.provider_id.clone(),
            base_url: self.base_url.clone(),
            api_key: self.api_key.clone(),
            model: self.model.clone(),
            slot: ModelGateway::slot_for_scene(scene),
        }
    }
}

fn default_strategy_for_scene(scene: AiScene) -> ContextStrategy {
    match scene {
        AiScene::ExemplarLearning | AiScene::DraftingAssist => ContextStrategy::LongContext,
        AiScene::KnowledgeLookup | AiScene::ResearchSynthesis => ContextStrategy::Hybrid,
    }
}

pub fn connectivity_status(
    db: &Database,
    active_scene: AiScene,
) -> AppResult<ConnectivityStatusDto> {
    let resolved = resolve_for_scene(db, active_scene)?;
    let llm_state = if crate::llm::providers::requires_api_key(&resolved.provider_id)
        && resolved.api_key.is_none()
    {
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

    let minimax_configured =
        credentials::has_secret(crate::credentials::MINIMAX_CREDENTIAL_SERVICE);
    let prefs = crate::llm::web_search_config::load(db)?;
    let effective_backend =
        crate::llm::search_web::expected_search_backend_for_connectivity(&prefs)
            .as_str()
            .to_string();
    let search_api = SearchApiConnectivityDto {
        minimax_configured,
        effective_backend,
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
        search_api,
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
    fn default_routing_has_four_scenes() {
        let c = deepseek_defaults();
        assert_eq!(c.scenes.len(), 4);
    }

    #[test]
    fn resolve_uses_deepseek_flash_for_knowledge() {
        let db = Database::open_in_memory().expect("mem db");
        let resolved = resolve_for_scene(&db, AiScene::KnowledgeLookup).expect("resolve");
        assert_eq!(resolved.provider_id, "deepseek");
        assert_eq!(resolved.model, "deepseek-v4-flash");
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
    fn migrate_sets_schema_version_and_created_at() {
        let mut v = serde_json::json!({
            "version": 1,
            "providers": {},
            "scenes": {},
            "contextStrategy": {}
        });
        LlmRoutingConfig::migrate(&mut v).expect("migrate");
        assert_eq!(v["schemaVersion"], serde_json::json!(1));
        assert!(v["createdAt"].is_string());
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
        assert_eq!(v["schemaVersion"], serde_json::json!(1));
        assert_eq!(v["createdAt"], serde_json::json!(existing));
    }

    #[test]
    fn migrate_noop_when_already_current() {
        let mut v = serde_json::json!({
            "version": 1,
            "schemaVersion": 1,
            "createdAt": "2024-01-01T00:00:00Z",
            "providers": {},
            "scenes": {},
            "contextStrategy": {}
        });
        LlmRoutingConfig::migrate(&mut v).expect("migrate");
        assert_eq!(v["schemaVersion"], serde_json::json!(1));
    }

    #[test]
    fn default_config_has_schema_version() {
        let c = deepseek_defaults();
        assert_eq!(c.schema_version, 1);
        assert!(c.created_at.is_some());
    }
}
