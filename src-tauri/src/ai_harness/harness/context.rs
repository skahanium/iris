//! Harness cold-start context: environment, skills, initial messages.
//! Delegates persona resolution to persona_resolver and prompt assembly to prompt_builder.

use crate::ai_runtime::environment::{build_environment_map, EnvironmentInput};
use crate::ai_runtime::model_gateway::LlmMessage;
use crate::ai_runtime::prompt_builder::HarnessMessageInput;
use crate::ai_runtime::prompt_profile::PromptProfile;
use crate::ai_runtime::skills::{
    active_skill_allowed_tools, active_skills_for_prompt, inject_into_prompt,
};
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
    state: &AppState,
    scene: AiScene,
    environment: &str,
    cold_start_packets: &[ContextPacket],
    history: &[(String, String)],
    web_search_enabled: bool,
    skills_fragment: Option<&str>,
) -> Vec<LlmMessage> {
    let profile = PromptProfile::load(&state.db).unwrap_or_default();
    crate::ai_runtime::prompt_builder::build_initial_messages(
        &HarnessMessageInput {
            scene,
            environment,
            cold_start_packets,
            history,
            web_search_enabled,
            skills_fragment,
        },
        &profile,
    )
}

pub(crate) fn prepare_environment_and_skills(
    state: &AppState,
    scene: AiScene,
    note_path: Option<&str>,
    note_title: Option<&str>,
    selection_excerpt: Option<&str>,
    user_message: &str,
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
    let enabled_skills = active_skills_for_prompt(&vault, scene, Some(&state.db), user_message)?;
    let skills_prompt = inject_into_prompt(&enabled_skills, scene, user_message);
    Ok((env_text, skills_prompt))
}

pub(crate) fn resolve_active_skill_allowed_tools(
    state: &AppState,
    scene: AiScene,
    user_message: &str,
) -> AppResult<Vec<String>> {
    let vault = state.vault_path()?;
    active_skill_allowed_tools(&vault, scene, Some(&state.db), user_message)
}
