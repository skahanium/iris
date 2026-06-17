//! Harness cold-start context: environment, skills, initial messages.
//! Delegates persona resolution to persona_resolver and prompt assembly to prompt_builder.

use crate::ai_runtime::environment::{build_environment_map, EnvironmentInput};
use crate::ai_runtime::model_gateway::LlmMessage;
use crate::ai_runtime::prompt_builder::HarnessMessageInput;
use crate::ai_runtime::prompt_profile::PromptProfile;
use crate::ai_runtime::skills::{
    active_skill_allowed_tools, active_skills_for_prompt, inject_into_prompt, scan_all,
};
use crate::ai_runtime::ToolSpec;
use crate::ai_runtime::{AiScene, ContextPacket, SkillActivationPlanSummary};
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

pub(crate) struct EnvironmentAndSkillsInput<'a> {
    pub(crate) scene: AiScene,
    pub(crate) note_path: Option<&'a str>,
    pub(crate) note_title: Option<&'a str>,
    pub(crate) selection_excerpt: Option<&'a str>,
    pub(crate) user_message: &'a str,
    pub(crate) scene_tools: &'a [ToolSpec],
    pub(crate) web_search_enabled: bool,
    pub(crate) attachment_count: usize,
}

pub(crate) fn prepare_environment_and_skills_with_plan(
    state: &AppState,
    input: EnvironmentAndSkillsInput<'_>,
    plan: Option<&SkillActivationPlanSummary>,
) -> AppResult<(String, String)> {
    if plan.is_none() {
        let vault = state.vault_path()?;
        let env_text = build_environment_map(
            &state.db,
            &vault,
            &EnvironmentInput {
                scene: input.scene,
                note_path: input.note_path,
                note_title: input.note_title,
                selection_excerpt: input.selection_excerpt,
                tools: input.scene_tools,
                web_search_enabled: input.web_search_enabled,
                attachment_count: input.attachment_count,
            },
        )?;
        let enabled_skills =
            active_skills_for_prompt(&vault, input.scene, Some(&state.db), input.user_message)?;
        let skills_prompt =
            inject_into_prompt(&vault, &enabled_skills, input.scene, input.user_message);
        return Ok((env_text, skills_prompt));
    }
    let vault = state.vault_path()?;
    let env_text = build_environment_map(
        &state.db,
        &vault,
        &EnvironmentInput {
            scene: input.scene,
            note_path: input.note_path,
            note_title: input.note_title,
            selection_excerpt: input.selection_excerpt,
            tools: input.scene_tools,
            web_search_enabled: input.web_search_enabled,
            attachment_count: input.attachment_count,
        },
    )?;
    let plan = plan.expect("checked above");
    let selected: Vec<_> = scan_all(&vault)?
        .into_iter()
        .filter(|skill| {
            plan.activated_skills.iter().any(|active| {
                active.name == skill.name
                    && active
                        .scope
                        .eq_ignore_ascii_case(&format!("{:?}", skill.scope))
            })
        })
        .collect();
    let skills_prompt = inject_into_prompt(&vault, &selected, input.scene, input.user_message);
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

pub(crate) fn resolve_active_skill_allowed_tools_with_plan(
    state: &AppState,
    scene: AiScene,
    user_message: &str,
    plan: Option<&SkillActivationPlanSummary>,
) -> AppResult<Vec<String>> {
    if let Some(plan) = plan {
        return Ok(plan.allowed_tools());
    }
    resolve_active_skill_allowed_tools(state, scene, user_message)
}
