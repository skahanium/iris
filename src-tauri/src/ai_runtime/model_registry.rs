//! Model capability-slot registry.
//!
//! 不在架构中硬编码厂商/模型名。模型选择通过能力槽位 (slot) 路由，
//! 运行时根据用户设置和 provider 可用性解析具体 provider/model。

use serde::{Deserialize, Serialize};

// ─── Capability Slot ─────────────────────────────────────

/// 能力槽位：描述"需要什么类型的模型"，而非"用哪个模型"。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySlot {
    /// 快速任务：续写、短改写、分类
    Fast,
    /// 写作质量：段落生成、风格模仿
    Writer,
    /// 深度推理：论证链、复杂研究
    Reasoner,
    /// 长上下文：长范文分析
    LongContext,
    /// 本地嵌入向量
    Embedding,
    /// 检索重排
    Reranker,
    /// 本地私有模型
    LocalPrivate,
}

// ─── Model Capability Profile ────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilityProfile {
    pub slot: CapabilitySlot,
    pub provider: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,
    #[serde(default)]
    pub supports_tools: bool,
    #[serde(default)]
    pub supports_streaming: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_json_schema: Option<bool>,
    #[serde(default)]
    pub privacy_level: PrivacyLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum PrivacyLevel {
    Local,
    #[default]
    External,
}

// ─── Registry ────────────────────────────────────────────

/// 模型注册表：维护 slot → profile 的映射。
///
/// 在应用启动时构造，从用户设置和预置 provider 信息中填充。
/// 查询时返回该 slot 当前激活的 profile。
#[derive(Debug, Clone, Default)]
pub struct ModelRegistry {
    profiles: Vec<ModelCapabilityProfile>,
    /// provider → (default_model, supports_tools, supports_streaming)
    providers: Vec<ProviderInfo>,
}

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub default_model: String,
    pub supports_tools: bool,
    pub supports_streaming: bool,
    pub privacy_level: PrivacyLevel,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 从现有 provider 列表初始化 registry。
    /// 当前阶段使用硬编码映射，后续从 user_profile 表读取用户偏好。
    pub fn from_providers(providers: Vec<ProviderInfo>) -> Self {
        let profiles = Self::build_default_profiles(&providers);
        Self {
            profiles,
            providers,
        }
    }

    /// 根据能力槽位解析当前激活的 model profile。
    /// 优先使用用户设置中的偏好；若无设置，使用内置默认。
    pub fn resolve(&self, slot: CapabilitySlot) -> Option<&ModelCapabilityProfile> {
        // 后续阶段：先查 user_profile 中 slot→provider/model 映射
        // 当前阶段：返回预置默认
        self.profiles.iter().find(|p| p.slot == slot)
    }

    /// 获取所有已注册的 provider 信息。
    pub fn list_providers(&self) -> &[ProviderInfo] {
        &self.providers
    }

    /// 按 slot 获取所有可用 profile（用于设置页展示选项）。
    pub fn profiles_for_slot(&self, slot: CapabilitySlot) -> Vec<&ModelCapabilityProfile> {
        self.profiles.iter().filter(|p| p.slot == slot).collect()
    }

    // ─── private helpers ──────────────────────────────

    fn build_default_profiles(providers: &[ProviderInfo]) -> Vec<ModelCapabilityProfile> {
        // 为每个 provider 的默认模型创建 profile。
        // 首个支持 tools 的 external provider 覆盖 Fast/Writer/Reasoner 槽位。
        let external = providers.iter().find(|p| {
            p.supports_tools && p.supports_streaming && p.privacy_level == PrivacyLevel::External
        });
        let local = providers
            .iter()
            .find(|p| p.privacy_level == PrivacyLevel::Local && p.supports_streaming);

        let mut profiles = Vec::new();

        if let Some(ext) = external {
            profiles.push(ModelCapabilityProfile {
                slot: CapabilitySlot::Fast,
                provider: ext.id.clone(),
                model: ext.default_model.clone(),
                context_window: Some(128_000),
                supports_tools: ext.supports_tools,
                supports_streaming: ext.supports_streaming,
                supports_json_schema: None,
                privacy_level: PrivacyLevel::External,
            });
            profiles.push(ModelCapabilityProfile {
                slot: CapabilitySlot::Writer,
                provider: ext.id.clone(),
                model: ext.default_model.clone(),
                context_window: Some(128_000),
                supports_tools: ext.supports_tools,
                supports_streaming: ext.supports_streaming,
                supports_json_schema: None,
                privacy_level: PrivacyLevel::External,
            });
            profiles.push(ModelCapabilityProfile {
                slot: CapabilitySlot::Reasoner,
                provider: ext.id.clone(),
                model: ext.default_model.clone(),
                context_window: Some(128_000),
                supports_tools: ext.supports_tools,
                supports_streaming: ext.supports_streaming,
                supports_json_schema: None,
                privacy_level: PrivacyLevel::External,
            });
            profiles.push(ModelCapabilityProfile {
                slot: CapabilitySlot::LongContext,
                provider: ext.id.clone(),
                model: ext.default_model.clone(),
                context_window: Some(1_000_000),
                supports_tools: ext.supports_tools,
                supports_streaming: ext.supports_streaming,
                supports_json_schema: None,
                privacy_level: PrivacyLevel::External,
            });
        }

        if let Some(loc) = local {
            profiles.push(ModelCapabilityProfile {
                slot: CapabilitySlot::LocalPrivate,
                provider: loc.id.clone(),
                model: loc.default_model.clone(),
                context_window: Some(128_000),
                supports_tools: loc.supports_tools,
                supports_streaming: loc.supports_streaming,
                supports_json_schema: None,
                privacy_level: PrivacyLevel::Local,
            });
        }

        // Embedding 和 Reranker 槽位当前固定为本地 fastembed
        profiles.push(ModelCapabilityProfile {
            slot: CapabilitySlot::Embedding,
            provider: "local".into(),
            model: "fastembed/AllMiniLML6V2".into(),
            context_window: None,
            supports_tools: false,
            supports_streaming: false,
            supports_json_schema: None,
            privacy_level: PrivacyLevel::Local,
        });
        profiles.push(ModelCapabilityProfile {
            slot: CapabilitySlot::Reranker,
            provider: "local".into(),
            model: "score-fusion".into(),
            context_window: None,
            supports_tools: false,
            supports_streaming: false,
            supports_json_schema: None,
            privacy_level: PrivacyLevel::Local,
        });

        profiles
    }
}

// ─── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_providers() -> Vec<ProviderInfo> {
        vec![
            ProviderInfo {
                id: "deepseek".into(),
                name: "DeepSeek".into(),
                default_model: "deepseek-chat".into(),
                supports_tools: true,
                supports_streaming: true,
                privacy_level: PrivacyLevel::External,
            },
            ProviderInfo {
                id: "ollama".into(),
                name: "Ollama".into(),
                default_model: "llama3".into(),
                supports_tools: true,
                supports_streaming: true,
                privacy_level: PrivacyLevel::Local,
            },
        ]
    }

    #[test]
    fn registry_resolves_fast_slot() {
        let reg = ModelRegistry::from_providers(test_providers());
        let profile = reg.resolve(CapabilitySlot::Fast);
        assert!(profile.is_some());
        let p = profile.unwrap();
        assert_eq!(p.provider, "deepseek");
        assert!(p.supports_streaming);
    }

    #[test]
    fn registry_resolves_local_private() {
        let reg = ModelRegistry::from_providers(test_providers());
        let profile = reg.resolve(CapabilitySlot::LocalPrivate);
        assert!(profile.is_some());
        let p = profile.unwrap();
        assert_eq!(p.provider, "ollama");
        assert_eq!(p.privacy_level, PrivacyLevel::Local);
    }

    #[test]
    fn embedding_slot_always_local() {
        let reg = ModelRegistry::from_providers(test_providers());
        let profile = reg.resolve(CapabilitySlot::Embedding);
        assert!(profile.is_some());
        assert_eq!(profile.unwrap().privacy_level, PrivacyLevel::Local);
    }

    #[test]
    fn empty_registry_returns_none() {
        let reg = ModelRegistry::from_providers(vec![]);
        assert!(reg.resolve(CapabilitySlot::Fast).is_none());
    }
}
