//! Harness cold-start context: environment, skills, initial messages.
//! Delegates persona resolution to persona_resolver and prompt assembly to prompt_builder.

use crate::ai_runtime::agent_task_policy::AgentTaskPolicy;
use crate::ai_runtime::environment::{build_environment_map, EnvironmentInput};
use crate::ai_runtime::model_gateway::LlmMessage;
use crate::ai_runtime::prompt_builder::HarnessMessageInput;
use crate::ai_runtime::prompt_profile::PromptProfile;
use crate::ai_runtime::skills::{active_skills_for_task_prompt, inject_into_prompt, scan_all};
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

pub(crate) struct InitialMessagesInput<'a> {
    pub(crate) scene: AiScene,
    pub(crate) session_id: i64,
    pub(crate) task_policy: &'a AgentTaskPolicy,
    pub(crate) environment: &'a str,
    pub(crate) cold_start_packets: &'a [ContextPacket],
    pub(crate) history: &'a [(String, String)],
    pub(crate) web_search_enabled: bool,
    pub(crate) skills_fragment: Option<&'a str>,
}

pub(crate) fn build_initial_messages(
    state: &AppState,
    input: InitialMessagesInput<'_>,
) -> Vec<LlmMessage> {
    let profile = PromptProfile::load(&state.db).unwrap_or_default();
    let mut history = Vec::new();
    if let Ok(Some(memory)) = crate::ai_runtime::conversation_memory::build_memory_system_message(
        &state.db,
        input.session_id,
    ) {
        history.push(memory);
    }
    history.extend(input.history.iter().cloned());
    crate::ai_runtime::prompt_builder::build_initial_messages(
        &HarnessMessageInput {
            scene: input.scene,
            task_policy: input.task_policy,
            environment: input.environment,
            cold_start_packets: input.cold_start_packets,
            history: &history,
            web_search_enabled: input.web_search_enabled,
            skills_fragment: input.skills_fragment,
        },
        &profile,
    )
}

pub(crate) struct EnvironmentAndSkillsInput<'a> {
    pub(crate) scene: AiScene,
    pub(crate) task_policy: &'a AgentTaskPolicy,
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
                task_policy: input.task_policy,
                note_path: input.note_path,
                note_title: input.note_title,
                selection_excerpt: input.selection_excerpt,
                tools: input.scene_tools,
                web_search_enabled: input.web_search_enabled,
                attachment_count: input.attachment_count,
            },
        )?;
        let enabled_skills = active_skills_for_task_prompt(
            &vault,
            input.task_policy.intent,
            Some(&state.db),
            input.user_message,
            &[],
        )?;
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
            task_policy: input.task_policy,
            note_path: input.note_path,
            note_title: input.note_title,
            selection_excerpt: input.selection_excerpt,
            tools: input.scene_tools,
            web_search_enabled: input.web_search_enabled,
            attachment_count: input.attachment_count,
        },
    )?;
    let plan = plan.expect("checked above");
    let mut selected: Vec<_> = scan_all(&vault)?
        .into_iter()
        .filter_map(|mut skill| {
            let active = plan.activated_skills.iter().find(|active| {
                active.name == skill.name
                    && active
                        .scope
                        .eq_ignore_ascii_case(&format!("{:?}", skill.scope))
            })?;
            let _ = crate::ai_runtime::skills::filter_skill_content_to_injected_sections(
                &mut skill,
                &active.injected_sections,
            );
            Some(skill)
        })
        .collect();
    selected.retain(|skill| !skill.content.trim().is_empty());
    let skills_prompt = inject_into_prompt(&vault, &selected, input.scene, input.user_message);
    Ok((env_text, skills_prompt))
}
