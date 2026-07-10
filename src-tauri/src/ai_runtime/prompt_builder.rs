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

/// Structured persisted session history for prompt assembly.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    pub seq: i64,
    pub role: String,
    pub content: String,
    pub tool_calls: Option<serde_json::Value>,
    pub evidence_packets: Option<serde_json::Value>,
    pub content_hash: Option<String>,
}

impl HistoryEntry {
    pub fn from_role_content(seq: i64, role: &str, content: &str) -> Self {
        Self {
            seq,
            role: role.to_string(),
            content: content.to_string(),
            tool_calls: None,
            evidence_packets: None,
            content_hash: None,
        }
    }
}

/// Token-aware knobs for assembling session history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryAssemblyPolicy {
    pub input_budget: Option<usize>,
    pub model_context_window: Option<usize>,
    pub max_tool_summary_tokens: usize,
    pub preserve_recent_turns: usize,
}

impl Default for HistoryAssemblyPolicy {
    fn default() -> Self {
        Self {
            input_budget: None,
            model_context_window: None,
            max_tool_summary_tokens: 500,
            preserve_recent_turns: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolTurnSummary {
    pub assistant_content: String,
    pub tool_name: String,
    pub args_summary: String,
    pub result_summary: String,
    pub status: String,
    pub evidence_ids: Vec<String>,
}

pub struct HistoryAssembler;

impl HistoryAssembler {
    pub fn assemble(
        legacy_history: &[(String, String)],
        structured_history: &[HistoryEntry],
        policy: HistoryAssemblyPolicy,
    ) -> Vec<(String, String)> {
        if structured_history.is_empty() {
            use crate::ai_runtime::harness_support::compress_history_messages;
            return compress_history_messages(legacy_history);
        }

        let mut out = Vec::new();
        let mut summaries = Vec::new();
        let start = structured_history
            .len()
            .saturating_sub(policy.preserve_recent_turns.max(1));
        let mut last_assistant: Option<&HistoryEntry> = None;
        for entry in &structured_history[start..] {
            match entry.role.as_str() {
                "system" if entry.content.contains("## ConversationMemory") => {
                    out.push((entry.role.clone(), entry.content.clone()));
                }
                "assistant" => {
                    last_assistant = Some(entry);
                    if !entry.content.trim().is_empty() {
                        out.push((entry.role.clone(), entry.content.clone()));
                    }
                }
                "tool" => {
                    if last_assistant.and_then(extract_first_tool_call).is_some() {
                        summaries.push(summarize_tool_entry(entry, last_assistant, policy));
                    }
                }
                "user" => out.push((entry.role.clone(), entry.content.clone())),
                _ => {}
            }
        }

        if !summaries.is_empty() {
            tracing::debug!(
                reason_code = "history_tool_summary_inserted",
                tool_summary_count = summaries.len(),
                "inserted historical tool turn summaries"
            );
            let body = summaries
                .into_iter()
                .map(format_tool_summary)
                .collect::<Vec<_>>()
                .join("\n");
            out.insert(
                0,
                ("system".to_string(), format!("## 历史工具结果摘要\n{body}")),
            );
        }
        out
    }
}

fn summarize_tool_entry(
    entry: &HistoryEntry,
    assistant: Option<&HistoryEntry>,
    policy: HistoryAssemblyPolicy,
) -> ToolTurnSummary {
    let (tool_name, args_summary) = assistant
        .and_then(extract_first_tool_call)
        .unwrap_or_else(|| ("tool".to_string(), String::new()));
    let status = infer_tool_status(&entry.content);
    let mut result_summary = summarize_tool_result(&tool_name, &entry.content);
    let marker = "\n...（已按上下文预算截断）";
    result_summary = crate::ai_runtime::harness_support::truncate_text_to_token_budget(
        &result_summary,
        policy.max_tool_summary_tokens.max(1),
        marker,
    );
    ToolTurnSummary {
        assistant_content: assistant
            .map(|entry| entry.content.clone())
            .unwrap_or_default(),
        tool_name,
        args_summary,
        result_summary,
        status,
        evidence_ids: extract_evidence_ids(entry.evidence_packets.as_ref()),
    }
}

fn extract_first_tool_call(entry: &HistoryEntry) -> Option<(String, String)> {
    let calls = entry.tool_calls.as_ref()?.as_array()?;
    let first = calls.first()?;
    let function = first.get("function")?;
    let name = function.get("name")?.as_str()?.to_string();
    let args = function
        .get("arguments")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    Some((name, summarize_jsonish_text(args, 180)))
}

fn infer_tool_status(content: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(content) {
        Ok(value) => {
            if value.get("error").is_some()
                || value.get("failure_class").is_some()
                || value.get("success").and_then(|v| v.as_bool()) == Some(false)
            {
                "error".to_string()
            } else {
                "ok".to_string()
            }
        }
        Err(_) => "ok".to_string(),
    }
}

fn summarize_tool_result(tool_name: &str, content: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
        return summarize_jsonish_text(content, 1_200);
    };
    if let Some(results) = value.get("results").and_then(|v| v.as_array()) {
        return results
            .iter()
            .take(5)
            .map(|item| {
                let title = item
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("untitled");
                let locator = item
                    .get("url")
                    .or_else(|| item.get("source_path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let snippet = item
                    .get("snippet")
                    .or_else(|| item.get("excerpt"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                format!(
                    "- {title} {locator}: {}",
                    summarize_jsonish_text(snippet, 240)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    if matches!(
        tool_name,
        "vault_create_note"
            | "vault_rename_move"
            | "vault_delete_to_trash"
            | "vault_asset_write"
            | "insert_text_at_cursor"
            | "replace_selection"
    ) {
        return summarize_write_result(&value);
    }
    summarize_jsonish_text(&value.to_string(), 1_200)
}

fn summarize_write_result(value: &serde_json::Value) -> String {
    let status = value
        .get("status")
        .or_else(|| value.get("result").and_then(|v| v.get("status")))
        .and_then(|v| v.as_str())
        .unwrap_or("ok");
    let path = value
        .get("path")
        .or_else(|| value.get("target_path"))
        .or_else(|| value.get("result").and_then(|v| v.get("path")))
        .and_then(|v| v.as_str())
        .unwrap_or("目标未声明");
    let error = value
        .get("error")
        .and_then(|v| v.as_str())
        .map(|err| format!("; error={}", summarize_jsonish_text(err, 160)))
        .unwrap_or_default();
    format!("status={status}; path={path}{error}")
}

fn summarize_jsonish_text(text: &str, max_chars: usize) -> String {
    let sanitized =
        crate::ai_runtime::trace::redact_classified_leaks(text).replace(['\n', '\r'], " ");
    sanitized.chars().take(max_chars).collect()
}

fn extract_evidence_ids(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("id")
                        .and_then(|id| id.as_str())
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn format_tool_summary(summary: ToolTurnSummary) -> String {
    let evidence = if summary.evidence_ids.is_empty() {
        String::new()
    } else {
        format!("; evidence_ids={}", summary.evidence_ids.join(","))
    };
    let args = if summary.args_summary.is_empty() {
        String::new()
    } else {
        format!("; args={}", summary.args_summary)
    };
    format!(
        "- tool={}; status={}{}{}; result={}",
        summary.tool_name, summary.status, args, evidence, summary.result_summary
    )
}
/// Inputs for the harness initial message construction.
#[derive(Debug, Clone)]
pub struct HarnessMessageInput<'a> {
    pub scene: AiScene,
    pub task_policy: &'a AgentTaskPolicy,
    pub environment: &'a str,
    pub cold_start_packets: &'a [ContextPacket],
    pub history: &'a [(String, String)],
    pub structured_history: &'a [HistoryEntry],
    pub history_policy: HistoryAssemblyPolicy,
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

    // History messages: convert persisted tool-role rows into safe summaries instead of replaying orphan tool messages.
    let compressed = HistoryAssembler::assemble(
        input.history,
        input.structured_history,
        input.history_policy,
    );
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
            structured_history: &[],
            history_policy: HistoryAssemblyPolicy::default(),
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
    fn harness_initial_messages_summarize_complete_tool_turns() {
        let profile = PromptProfile::default();
        let policy = legacy_policy(AiScene::KnowledgeLookup, true);
        let tool_calls = serde_json::json!([
            {
                "id": "call-search-1",
                "type": "function",
                "function": { "name": "web_search", "arguments": "{\"query\":\"Rust async patterns\"}" }
            }
        ]);
        let history = vec![
            HistoryEntry::from_role_content(1, "user", "搜索 Rust async patterns"),
            HistoryEntry {
                seq: 2,
                role: "assistant".into(),
                content: "我会搜索一下。".into(),
                tool_calls: Some(tool_calls),
                evidence_packets: None,
                content_hash: Some("assistant-hash".into()),
            },
            HistoryEntry::from_role_content(
                3,
                "tool",
                r#"{"results":[{"title":"Tokio tutorial","url":"https://tokio.rs","snippet":"Async runtime patterns"}]}"#,
            ),
            HistoryEntry::from_role_content(4, "assistant", "第一个结果是 Tokio tutorial。"),
            HistoryEntry::from_role_content(5, "user", "第一个结果是什么？"),
        ];
        let input = HarnessMessageInput {
            scene: AiScene::KnowledgeLookup,
            task_policy: &policy,
            environment: "",
            cold_start_packets: &[],
            history: &[],
            structured_history: &history,
            history_policy: HistoryAssemblyPolicy {
                input_budget: Some(8_000),
                model_context_window: Some(16_000),
                max_tool_summary_tokens: 120,
                preserve_recent_turns: 4,
            },
            web_search_enabled: true,
            skills_fragment: None,
        };

        let joined = build_initial_messages(&input, &profile)
            .into_iter()
            .map(|message| message.content.text_content())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(joined.contains("## 历史工具结果摘要"));
        assert!(joined.contains("web_search"));
        assert!(joined.contains("Tokio tutorial"));
        assert!(joined.contains("https://tokio.rs"));
        assert!(joined.contains("第一个结果是什么？"));
    }

    #[test]
    fn harness_initial_messages_truncate_tool_result_summaries() {
        let profile = PromptProfile::default();
        let policy = legacy_policy(AiScene::KnowledgeLookup, true);
        let long_body = "重要内容".repeat(2_000);
        let history = vec![
            HistoryEntry {
                seq: 1,
                role: "assistant".to_string(),
                content: "读取笔记。".to_string(),
                tool_calls: Some(
                    serde_json::json!([{"function":{"name":"read_note","arguments":"{\"path\":\"note.md\"}"}}]),
                ),
                evidence_packets: None,
                content_hash: None,
            },
            HistoryEntry::from_role_content(2, "tool", &long_body),
            HistoryEntry::from_role_content(3, "user", "概括刚才读到的内容"),
        ];
        let input = HarnessMessageInput {
            scene: AiScene::KnowledgeLookup,
            task_policy: &policy,
            environment: "",
            cold_start_packets: &[],
            history: &[],
            structured_history: &history,
            history_policy: HistoryAssemblyPolicy {
                input_budget: Some(2_000),
                model_context_window: Some(4_000),
                max_tool_summary_tokens: 40,
                preserve_recent_turns: 3,
            },
            web_search_enabled: true,
            skills_fragment: None,
        };

        let joined = build_initial_messages(&input, &profile)
            .into_iter()
            .map(|message| message.content.text_content())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(joined.contains("已按上下文预算截断"));
        assert!(joined.len() < long_body.len() / 4);
    }
    #[test]
    fn harness_initial_messages_skip_orphan_tool_messages() {
        let profile = PromptProfile::default();
        let policy = legacy_policy(AiScene::KnowledgeLookup, true);
        let history = vec![
            HistoryEntry::from_role_content(1, "tool", r#"{"content":"orphan"}"#),
            HistoryEntry::from_role_content(2, "user", "继续"),
        ];
        let input = HarnessMessageInput {
            scene: AiScene::KnowledgeLookup,
            task_policy: &policy,
            environment: "",
            cold_start_packets: &[],
            history: &[],
            structured_history: &history,
            history_policy: HistoryAssemblyPolicy::default(),
            web_search_enabled: true,
            skills_fragment: None,
        };

        let joined = build_initial_messages(&input, &profile)
            .into_iter()
            .map(|message| message.content.text_content())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(!joined.contains("## 历史工具结果摘要"));
        assert!(!joined.contains("orphan"));
        assert!(joined.contains("继续"));
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
            structured_history: &[],
            history_policy: HistoryAssemblyPolicy::default(),
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
