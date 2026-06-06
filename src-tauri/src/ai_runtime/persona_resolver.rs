//! PersonaResolver — resolves the effective persona for a harness run.
//!
//! Rules:
//! - When `PromptProfile.persona` is empty → use default persona identity;
//!   the name in that identity comes from `display_name` (fallback「砚」).
//! - When `PromptProfile.persona` is non-empty → user persona becomes primary;
//!   default「砚」only as product capability description, no longer forcing the name.
//! - `writing_style`, `language`, `custom_rules` are layered into the prompt.
//! - Scene focus only describes current task capability, does not override persona.

use crate::ai_runtime::prompt_profile::PromptProfile;
use crate::ai_runtime::AiScene;

/// Resolved persona text ready for prompt injection.
#[derive(Debug, Clone)]
pub struct ResolvedPersona {
    /// The persona identity block (who the assistant is).
    pub identity: String,
    /// Product/data principles (what Iris is).
    pub principles: String,
    /// Scene-specific capability focus.
    pub scene_focus: String,
    /// Web search instructions.
    pub web_instruction: String,
    /// Writing style guidance.
    pub writing_style: Option<String>,
    /// Response language.
    pub language: String,
    /// User-defined custom rules.
    pub custom_rules: Vec<String>,
}

/// Resolve the effective persona from a user profile and scene context.
pub fn resolve_persona(
    profile: &PromptProfile,
    scene: AiScene,
    web_search_enabled: bool,
) -> ResolvedPersona {
    let scene_focus = resolve_scene_focus(scene, web_search_enabled);
    let web_instruction = resolve_web_instruction(web_search_enabled);
    let language = if profile.language.is_empty() {
        "zh-CN".to_string()
    } else {
        profile.language.clone()
    };

    if profile.persona.is_empty() {
        // Default persona — identity name follows display_name
        ResolvedPersona {
            identity: default_identity(&effective_display_name(profile)),
            principles: default_principles(),
            scene_focus,
            web_instruction,
            writing_style: non_empty(&profile.writing_style),
            language,
            custom_rules: profile.custom_rules.clone(),
        }
    } else {
        // User-defined persona — user identity is primary
        ResolvedPersona {
            identity: profile.persona.clone(),
            principles: default_principles(),
            scene_focus,
            web_instruction,
            writing_style: non_empty(&profile.writing_style),
            language,
            custom_rules: profile.custom_rules.clone(),
        }
    }
}

fn effective_display_name(profile: &PromptProfile) -> String {
    let trimmed = profile.display_name.trim();
    if trimmed.is_empty() {
        "砚".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Default identity block; name comes from user display_name.
fn default_identity(display_name: &str) -> String {
    format!(
        "你是「{display_name}」，Iris 本地 Markdown 笔记本的 AI 助手。对用户你始终是同一个身份：\
         语气克制、清晰、可追溯。"
    )
}

/// Product/data principles — always included regardless of persona.
fn default_principles() -> String {
    "Iris 以用户的 .md 为唯一数据源；通过工具检索知识库、读取笔记。\
     在用户确认后修改文稿。"
        .into()
}

/// Scene-specific capability focus.
fn resolve_scene_focus(scene: AiScene, web_search_enabled: bool) -> String {
    match scene {
        AiScene::KnowledgeLookup => {
            if web_search_enabled {
                "知识查阅：先 search_hybrid 检索本地笔记，再 web_search（Token Plan 搜索）补充摘要；\
                 若摘要不足可对 1～2 个 HTTPS 链接调用 fetch_web_page 读取正文（需用户确认）；\
                 本地与网络证据结合、交叉引用，不可偏废"
                    .into()
            } else {
                "知识查阅：通过 search_hybrid 检索本地笔记；仅依据本地知识库回答".into()
            }
        }
        AiScene::ExemplarLearning => "范文学习：分析结构、句式与表达；模板保存需用户确认".into(),
        AiScene::DraftingAssist => {
            "文稿创作：低干扰辅助；写入笔记须用户确认；避免大段照搬范文".into()
        }
        AiScene::ResearchSynthesis => "研究综合：多材料交叉论证、证据缺口与引用核查".into(),
    }
}

/// Web search instruction block.
fn resolve_web_instruction(web_search_enabled: bool) -> String {
    if web_search_enabled {
        "联网已开启：web_search 使用 MiniMax Token Plan 搜索 API（返回标题/链接/摘要），\
         无需询问是否允许搜索。\n\
         需要页面正文时对明确 URL 调用 fetch_web_page（会弹出用户确认，每轮最多 1～2 次）。\n\
         禁止在正文中输出 DSML 或伪工具标记；必须通过工具 API 调用。\n\
         本地检索与网络搜索应结合、相互印证，不可只做其一。\n"
            .into()
    } else {
        "联网未开启——仅使用本地知识库，不要调用 web_search 或 fetch_web_page。\n".into()
    }
}

fn non_empty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Render the resolved persona into a full system prompt persona section.
pub fn render_persona(resolved: &ResolvedPersona) -> String {
    let mut parts = Vec::new();

    // Identity
    parts.push(resolved.identity.clone());

    // Principles
    parts.push(resolved.principles.clone());

    // Web instruction
    parts.push(resolved.web_instruction.clone());

    // Scene focus
    parts.push(format!("当前侧重：{}。\n", resolved.scene_focus));

    // Evidence citation instruction
    parts.push(
        "回答须基于工具结果与证据；引用时请使用证据包中提供的标签（如 [C1]、[W0]），\
         也可直接指明来源文件名或 URL；证据不足时直接说明，不编造。"
            .into(),
    );

    // Writing style
    if let Some(style) = &resolved.writing_style {
        parts.push(format!("写作风格：{style}"));
    }

    // Language
    parts.push(format!("回答语言：{}", resolved.language));

    // Custom rules
    if !resolved.custom_rules.is_empty() {
        let mut rules = String::from("用户自定义规则：\n");
        for rule in &resolved.custom_rules {
            rules.push_str(&format!("- {rule}\n"));
        }
        parts.push(rules);
    }

    parts.push(
        "Skills 管理：安装 skill 请调用 skills_install（registry/url/git/local），不要用 fetch_web_page 代替安装。\n\
         SkillHub：skills_install(source=registry, registry=skillhub, path_or_url=<skill名或页面URL>)。\n\
         查看已安装：skills_list。卸载/启停：skills_uninstall / skills_toggle，均需用户确认。\n\
         fetch_web_page 仅用于阅读文档，不写入 skills 目录。"
            .into(),
    );

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_persona_is_yan() {
        let profile = PromptProfile::default();
        let resolved = resolve_persona(&profile, AiScene::KnowledgeLookup, false);
        assert!(resolved.identity.contains("砚"));
        assert!(resolved.principles.contains(".md"));
        assert!(resolved.writing_style.is_none());
        assert_eq!(resolved.language, "zh-CN");
    }

    #[test]
    fn custom_persona_overrides_identity() {
        let profile = PromptProfile {
            persona: "自定义助手".into(),
            ..Default::default()
        };
        let resolved = resolve_persona(&profile, AiScene::KnowledgeLookup, false);
        assert_eq!(resolved.identity, "自定义助手");
        // Principles still present
        assert!(resolved.principles.contains("Iris"));
    }

    #[test]
    fn custom_persona_does_not_contain_yan() {
        let profile = PromptProfile {
            persona: "My Custom AI".into(),
            ..Default::default()
        };
        let resolved = resolve_persona(&profile, AiScene::KnowledgeLookup, false);
        let rendered = render_persona(&resolved);
        // The rendered prompt should NOT contain「砚」in the identity
        // (it may appear in principles as product description, but not as the name)
        assert!(!rendered.starts_with("你是「砚」"));
    }

    #[test]
    fn web_search_disabled_prohibits_tools() {
        let profile = PromptProfile::default();
        let resolved = resolve_persona(&profile, AiScene::KnowledgeLookup, false);
        assert!(resolved.web_instruction.contains("不要调用 web_search"));
    }

    #[test]
    fn web_search_enabled_allows_tools() {
        let profile = PromptProfile::default();
        let resolved = resolve_persona(&profile, AiScene::KnowledgeLookup, true);
        assert!(resolved.web_instruction.contains("web_search"));
        assert!(!resolved.web_instruction.contains("不要调用"));
    }

    #[test]
    fn writing_style_preserved() {
        let profile = PromptProfile {
            writing_style: "简洁明了".into(),
            ..Default::default()
        };
        let resolved = resolve_persona(&profile, AiScene::KnowledgeLookup, false);
        assert_eq!(resolved.writing_style, Some("简洁明了".into()));
    }

    #[test]
    fn custom_rules_preserved() {
        let profile = PromptProfile {
            custom_rules: vec!["规则一".into(), "规则二".into()],
            ..Default::default()
        };
        let resolved = resolve_persona(&profile, AiScene::KnowledgeLookup, false);
        assert_eq!(resolved.custom_rules.len(), 2);
    }

    #[test]
    fn scene_focus_varies_by_scene() {
        let profile = PromptProfile::default();
        let kl = resolve_persona(&profile, AiScene::KnowledgeLookup, false);
        let da = resolve_persona(&profile, AiScene::DraftingAssist, false);
        assert!(kl.scene_focus.contains("知识查阅"));
        assert!(da.scene_focus.contains("文稿创作"));
    }

    #[test]
    fn default_persona_uses_custom_display_name() {
        let profile = PromptProfile {
            display_name: "小鸢".into(),
            ..Default::default()
        };
        let resolved = resolve_persona(&profile, AiScene::KnowledgeLookup, false);
        assert!(resolved.identity.contains("小鸢"));
        assert!(!resolved.identity.contains("你是「砚」"));
    }

    #[test]
    fn render_persona_includes_all_sections() {
        let profile = PromptProfile {
            persona: "Test AI".into(),
            writing_style: "formal".into(),
            language: "en".into(),
            custom_rules: vec!["rule1".into()],
            ..Default::default()
        };
        let resolved = resolve_persona(&profile, AiScene::ResearchSynthesis, false);
        let rendered = render_persona(&resolved);
        assert!(rendered.contains("Test AI"));
        assert!(rendered.contains("Iris"));
        assert!(rendered.contains("formal"));
        assert!(rendered.contains("en"));
        assert!(rendered.contains("rule1"));
        assert!(rendered.contains("研究综合"));
    }
}
