//! PromptBuilder — unified prompt construction for harness and workflows.
//!
//! Assembles 7 layers into a cache-friendly multi-message system prompt:
//!
//! ```text
//! Layer 1: Persona (identity + principles + task focus + web instructions)
//! Layer 2: Product/Data Principles (already in Layer 1 for default persona)
//! Layer 3: Task Focus (already in Layer 1)
//! Layer 4: Tool Policy Summary
//! Layer 5: Active Skills
//! Layer 6: Evidence Packets
//! Layer 7: User Rules
//! ```
//!
//! Each layer is a separate `LlmMessage` with `System` role for cache-friendly layout.

use crate::ai_runtime::agent_task::AgentTaskKind;
use crate::ai_runtime::agent_task_policy::{
    intent_from_legacy_scene, AgentTaskPolicy, AgentTaskPolicyInput, AgentTaskScope,
};
use crate::ai_runtime::environment::{build_environment_map, EnvironmentInput};
use crate::ai_runtime::model_gateway::{LlmMessage, MessageRole, ModelGateway};
use crate::ai_runtime::persona_resolver::{
    render_persona, resolve_persona, resolve_persona_for_policy,
};
use crate::ai_runtime::prompt_profile::PromptProfile;
use crate::ai_runtime::{AiScene, ContextPacket, ToolSpec};
use crate::error::AppResult;
use std::path::Path;

const EVIDENCE_GAP_GUIDANCE: &str = "## 证据使用规则\n\n\
如果当前证据不足，必须先明确说明“当前证据不足”。\
可以给出模型常识层面的初步判断，但必须明确标注“未由当前证据支持”。\
法规、制度、政策、规范、医疗、法律、财务、版本事实、近期事实等高风险或时效性问题，不能把未检索到的内容当作事实依据。";

/// Inputs for the PromptBuilder.
#[derive(Debug, Clone)]
pub struct PromptBuildInput<'a> {
    pub scene: AiScene,
    pub web_search_enabled: bool,
    pub note_path: Option<&'a str>,
    pub note_title: Option<&'a str>,
    pub selection_excerpt: Option<&'a str>,
    pub tools: &'a [ToolSpec],
    pub cold_start_packets: &'a [ContextPacket],
    pub history: &'a [(String, String)],
    pub skills_fragment: Option<&'a str>,
}

/// Build the complete system prompt as a vector of messages.
///
/// Returns multiple `System` messages for cache-friendly layout:
/// 1. Persona + principles + task focus + web instructions + writing style + language + rules
/// 2. Environment (capabilities, document context, vault structure)
/// 3. Evidence packets (if any)
/// 4. Skills fragment (if any)
pub fn build_prompt_messages(
    db: &crate::storage::db::Database,
    vault: &Path,
    input: &PromptBuildInput<'_>,
    profile: &PromptProfile,
) -> AppResult<Vec<LlmMessage>> {
    let resolved = resolve_persona(profile, input.scene, input.web_search_enabled);
    let fallback_policy = legacy_policy(input.scene, input.web_search_enabled);
    let mut messages = Vec::new();

    // Layer 1-3,7: Persona (identity + principles + scene + web + style + language + rules)
    let persona_text = render_persona(&resolved);
    messages.push(LlmMessage {
        role: MessageRole::System,
        content: persona_text.into(),
        tool_call_id: None,
        tool_calls: None,
        ..Default::default()
    });

    // Layer 4: Environment (capabilities, document, vault, backlinks)
    let env_text = build_environment_map(
        db,
        vault,
        &EnvironmentInput {
            scene: input.scene,
            task_policy: &fallback_policy,
            note_path: input.note_path,
            note_title: input.note_title,
            selection_excerpt: input.selection_excerpt,
            tools: input.tools,
            web_search_enabled: input.web_search_enabled,
            attachment_count: 0,
        },
    )?;
    if !env_text.is_empty() {
        messages.push(LlmMessage {
            role: MessageRole::System,
            content: env_text.into(),
            tool_call_id: None,
            tool_calls: None,
            ..Default::default()
        });
    }

    messages.push(LlmMessage {
        role: MessageRole::System,
        content: EVIDENCE_GAP_GUIDANCE.into(),
        tool_call_id: None,
        tool_calls: None,
        ..Default::default()
    });

    // Layer 5: Active Skills
    if let Some(skills) = input.skills_fragment {
        if !skills.is_empty() {
            messages.push(LlmMessage {
                role: MessageRole::System,
                content: skills.into(),
                tool_call_id: None,
                tool_calls: None,
                ..Default::default()
            });
        }
    }

    // Layer 6: Evidence Packets
    if !input.cold_start_packets.is_empty() {
        let hint = ModelGateway::format_evidence_packets(input.cold_start_packets);
        messages.push(LlmMessage {
            role: MessageRole::System,
            content: format!(
                "## 本地知识库检索材料\n\n\
                 以下是从你的笔记中预检索到的相关材料，请认真参考并在回答中引用；\
                 同时结合工具检索与网络搜索交叉验证。\n\n{hint}"
            )
            .into(),
            tool_call_id: None,
            tool_calls: None,
            ..Default::default()
        });
    }

    Ok(messages)
}

/// Inputs for the harness initial message construction.
#[derive(Debug, Clone)]
pub struct HarnessMessageInput<'a> {
    pub scene: AiScene,
    pub task_policy: &'a AgentTaskPolicy,
    pub environment: &'a str,
    pub cold_start_packets: &'a [ContextPacket],
    pub history: &'a [(String, String)],
    pub web_search_enabled: bool,
    pub skills_fragment: Option<&'a str>,
}

/// Build the initial message array for the harness (system + history).
///
/// This is the main entry point used by `harness/context.rs`.
pub fn build_initial_messages(
    input: &HarnessMessageInput<'_>,
    profile: &PromptProfile,
) -> Vec<LlmMessage> {
    let resolved = resolve_persona_for_policy(profile, input.task_policy, input.skills_fragment);
    let mut messages = Vec::new();

    // Stable persona layer: keep dynamic environment and skills in later messages
    // so provider-side prefix caching can reuse the invariant prompt prefix.
    let persona_text = render_persona(&resolved);
    messages.push(LlmMessage {
        role: MessageRole::System,
        content: persona_text.into(),
        tool_call_id: None,
        tool_calls: None,
        ..Default::default()
    });

    if !input.environment.is_empty() {
        messages.push(LlmMessage {
            role: MessageRole::System,
            content: input.environment.into(),
            tool_call_id: None,
            tool_calls: None,
            ..Default::default()
        });
    }

    if let Some(skills) = input.skills_fragment {
        if !skills.is_empty() {
            messages.push(LlmMessage {
                role: MessageRole::System,
                content: skills.into(),
                tool_call_id: None,
                tool_calls: None,
                ..Default::default()
            });
        }
    }

    messages.push(LlmMessage {
        role: MessageRole::System,
        content: EVIDENCE_GAP_GUIDANCE.into(),
        tool_call_id: None,
        tool_calls: None,
        ..Default::default()
    });

    // Evidence packets
    if !input.cold_start_packets.is_empty() {
        let hint = ModelGateway::format_evidence_packets(input.cold_start_packets);
        messages.push(LlmMessage {
            role: MessageRole::System,
            content: format!(
                "## 本地知识库检索材料\n\n\
                 以下是从你的笔记中预检索到的相关材料，请认真参考并在回答中引用；\
                 同时结合工具检索与网络搜索交叉验证。\n\n{hint}"
            )
            .into(),
            tool_call_id: None,
            tool_calls: None,
            ..Default::default()
        });
    }

    // History messages (skip orphan tool rows — they lack tool_calls context)
    use crate::ai_runtime::harness_support::compress_history_messages;
    let compressed = compress_history_messages(input.history);
    for (role, content) in compressed {
        if role == "tool" {
            continue;
        }
        let r = match role.as_str() {
            "system" => MessageRole::System,
            "assistant" => MessageRole::Assistant,
            _ => MessageRole::User,
        };
        messages.push(LlmMessage {
            role: r,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
            ..Default::default()
        });
    }

    messages
}

fn legacy_policy(scene: AiScene, web_search_enabled: bool) -> AgentTaskPolicy {
    let intent = intent_from_legacy_scene(scene);
    AgentTaskPolicy::from_input(AgentTaskPolicyInput {
        intent,
        task_kind: AgentTaskKind::Lightweight,
        scope: AgentTaskScope::Vault,
        web_authorized: web_search_enabled,
        has_attachments: false,
        write_permission_required: false,
        research_depth: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::prompt_profile::PromptProfile;

    #[test]
    fn default_persona_in_prompt() {
        let profile = PromptProfile::default();
        let resolved = resolve_persona(&profile, AiScene::KnowledgeLookup, false);
        let rendered = render_persona(&resolved);
        assert!(rendered.contains("砚"));
        assert!(rendered.contains("Iris"));
        assert!(rendered.contains("知识查阅"));
    }

    #[test]
    fn custom_persona_in_prompt() {
        let profile = PromptProfile {
            persona: "Custom AI".into(),
            ..Default::default()
        };
        let resolved = resolve_persona(&profile, AiScene::KnowledgeLookup, false);
        let rendered = render_persona(&resolved);
        assert!(rendered.contains("Custom AI"));
        assert!(rendered.starts_with("Safety overlay"));
        // Should NOT start with「砚」
        assert!(!rendered.starts_with("你是「砚」"));
    }

    #[test]
    fn model_gateway_prompt_accepts_real_prompt_profile() {
        let profile = PromptProfile {
            persona: "Workflow Custom Persona".into(),
            writing_style: "terse".into(),
            language: "en".into(),
            custom_rules: vec!["Never claim unsupported facts".into()],
            ..Default::default()
        };
        let prompt = ModelGateway::build_system_prompt_with_profile(
            AiScene::DraftingAssist,
            &[],
            &[],
            false,
            &profile,
        );

        assert!(prompt.starts_with("Safety overlay"));
        assert!(prompt.contains("Workflow Custom Persona"));
        assert!(!prompt.starts_with("你是「砚」"));
        assert!(prompt.contains("terse"));
        assert!(prompt.contains("Never claim unsupported facts"));
    }

    #[test]
    fn web_search_disabled_in_prompt() {
        let profile = PromptProfile::default();
        let resolved = resolve_persona(&profile, AiScene::KnowledgeLookup, false);
        let rendered = render_persona(&resolved);
        assert!(rendered.contains("仅使用本地知识库"));
        assert!(rendered.contains("不要调用网络检索"));
        assert!(!rendered.contains("web_search"));
        assert!(!rendered.contains("fetch_web_page"));
    }

    #[test]
    fn web_search_enabled_in_prompt() {
        let profile = PromptProfile::default();
        let resolved = resolve_persona(&profile, AiScene::KnowledgeLookup, true);
        let rendered = render_persona(&resolved);
        assert!(rendered.contains("网络证据代理"));
        assert!(!rendered.contains("web_search"));
        assert!(!rendered.contains("fetch_web_page"));
    }

    #[test]
    fn skills_injected_in_prompt() {
        let profile = PromptProfile::default();
        let resolved = resolve_persona(&profile, AiScene::KnowledgeLookup, false);
        let rendered = render_persona(&resolved);
        // Skills are injected separately, not in persona
        assert!(!rendered.contains("已激活 Skills"));
    }

    #[test]
    fn legacy_task_focus_correct_per_scene() {
        let profile = PromptProfile::default();
        for (scene, expected) in [
            (AiScene::KnowledgeLookup, "知识查阅"),
            (AiScene::DraftingAssist, "文稿创作"),
            (AiScene::ResearchSynthesis, "研究综合"),
        ] {
            let resolved = resolve_persona(&profile, scene, false);
            let rendered = render_persona(&resolved);
            assert!(
                rendered.contains(expected),
                "scene {scene:?} should contain '{expected}'"
            );
        }
    }

    #[test]
    fn writing_style_and_language_in_prompt() {
        let profile = PromptProfile {
            writing_style: "简洁".into(),
            language: "en".into(),
            ..Default::default()
        };
        let resolved = resolve_persona(&profile, AiScene::KnowledgeLookup, false);
        let rendered = render_persona(&resolved);
        assert!(rendered.contains("简洁"));
        assert!(rendered.contains("en"));
    }

    #[test]
    fn custom_rules_in_prompt() {
        let profile = PromptProfile {
            custom_rules: vec!["Always cite sources".into()],
            ..Default::default()
        };
        let resolved = resolve_persona(&profile, AiScene::KnowledgeLookup, false);
        let rendered = render_persona(&resolved);
        assert!(rendered.contains("Always cite sources"));
    }

    #[test]
    fn harness_initial_messages_keep_stable_persona_separate_from_dynamic_layers() {
        let profile = PromptProfile::default();
        let policy = legacy_policy(AiScene::KnowledgeLookup, false);
        let input = HarnessMessageInput {
            scene: AiScene::KnowledgeLookup,
            task_policy: &policy,
            environment: "Environment: 当前笔记标题 A",
            cold_start_packets: &[],
            history: &[("user".to_string(), "问题".to_string())],
            web_search_enabled: false,
            skills_fragment: Some("Skill overlay: active skill"),
        };

        let messages = build_initial_messages(&input, &profile);

        assert!(messages[0].content.as_str().unwrap().contains("Iris"));
        assert!(!messages[0]
            .content
            .as_str()
            .unwrap()
            .contains("当前笔记标题 A"));
        assert_eq!(
            messages[1].content.as_str(),
            Some("Environment: 当前笔记标题 A")
        );
        assert_eq!(
            messages[2].content.as_str(),
            Some("Skill overlay: active skill")
        );
        assert!(messages[3]
            .content
            .as_str()
            .unwrap()
            .contains("证据使用规则"));
        assert_eq!(messages[4].content.as_str(), Some("问题"));
    }

    #[test]
    fn harness_initial_messages_require_explicit_evidence_gap_labels() {
        let profile = PromptProfile::default();
        let policy = legacy_policy(AiScene::KnowledgeLookup, false);
        let input = HarnessMessageInput {
            scene: AiScene::KnowledgeLookup,
            task_policy: &policy,
            environment: "",
            cold_start_packets: &[],
            history: &[("user".to_string(), "问题".to_string())],
            web_search_enabled: false,
            skills_fragment: None,
        };

        let joined = build_initial_messages(&input, &profile)
            .into_iter()
            .map(|message| message.content.text_content())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(joined.contains("当前证据不足"));
        assert!(joined.contains("未由当前证据支持"));
    }
}
