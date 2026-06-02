//! Harness cold-start context: environment, skills, initial messages.

use crate::ai_runtime::environment::{build_environment_map, EnvironmentInput};
use crate::ai_runtime::harness_support::compress_history_messages;
use crate::ai_runtime::model_gateway::{LlmMessage, MessageRole, ModelGateway};
use crate::ai_runtime::skills::{inject_into_prompt, scan_all};
use crate::ai_runtime::ToolSpec;
use crate::ai_runtime::{AiScene, ContextPacket};
use crate::app::AppState;
use crate::error::AppResult;

pub(crate) fn resolve_file_id(state: &AppState, note_path: Option<&str>) -> AppResult<Option<i64>> {
    let Some(path) = note_path else {
        return Ok(None);
    };
    state.db.with_conn(|conn| {
        Ok(conn
            .query_row("SELECT id FROM files WHERE path = ?1", [path], |r| {
                r.get::<_, i64>(0)
            })
            .ok())
    })
}

pub(crate) fn build_initial_messages(
    scene: AiScene,
    environment: &str,
    cold_start_packets: &[ContextPacket],
    history: &[(String, String)],
    web_search_enabled: bool,
    skills_fragment: Option<&str>,
) -> Vec<LlmMessage> {
    let persona = ModelGateway::unified_persona(scene, web_search_enabled);
    let mut system_content = format!("{persona}\n\n{environment}");
    if let Some(skills) = skills_fragment {
        if !skills.is_empty() {
            system_content.push_str("\n\n");
            system_content.push_str(skills);
        }
    }
    let mut messages = vec![LlmMessage {
        role: MessageRole::System,
        content: system_content,
        tool_call_id: None,
        tool_calls: None,
    }];

    if !cold_start_packets.is_empty() {
        let hint = ModelGateway::format_evidence_packets(cold_start_packets);
        messages.push(LlmMessage {
            role: MessageRole::System,
            content: format!(
                "## 本地知识库检索材料\n\n\
                 以下是从你的笔记中预检索到的相关材料，请认真参考并在回答中引用；\
                 同时结合工具检索与网络搜索交叉验证。\n\n{hint}"
            ),
            tool_call_id: None,
            tool_calls: None,
        });
    }

    let compressed = compress_history_messages(history);
    for (role, content) in compressed {
        let r = match role.as_str() {
            "assistant" => MessageRole::Assistant,
            "tool" => MessageRole::Tool,
            _ => MessageRole::User,
        };
        messages.push(LlmMessage {
            role: r,
            content,
            tool_call_id: None,
            tool_calls: None,
        });
    }

    messages
}

pub(crate) fn prepare_environment_and_skills(
    state: &AppState,
    scene: AiScene,
    note_path: Option<&str>,
    note_title: Option<&str>,
    selection_excerpt: Option<&str>,
    scene_tools: &[ToolSpec],
) -> AppResult<(String, String)> {
    let vault = state.vault_path()?;
    let env_text = build_environment_map(
        &state.db,
        &vault,
        &EnvironmentInput {
            scene,
            note_path,
            note_title,
            selection_excerpt,
            tools: scene_tools,
        },
    )?;
    let all_skills = scan_all(&vault)?;
    let enabled_skills: Vec<_> = all_skills.into_iter().filter(|s| s.enabled).collect();
    let skills_prompt = inject_into_prompt(&enabled_skills, scene);
    Ok((env_text, skills_prompt))
}
