use std::path::Path;

use crate::ai_runtime::AiScene;

use super::{skills_for_scene, workspace_status_for_skill, SkillEntry};

const MAX_SKILL_PROMPT_BODY_CHARS: usize = 12_000;

/// Build system prompt fragment from enabled skills.
pub fn inject_into_prompt(
    vault: &Path,
    skills: &[SkillEntry],
    scene: AiScene,
    user_message: &str,
) -> String {
    let matched = skills_for_scene(skills, scene, user_message);
    if matched.is_empty() {
        return String::new();
    }
    let mut block = String::from("## Activated Skills\n\n");
    block.push_str(
        "If a skill references files under `references/`, `resources/`, or `assets/`, call `skills_read_resource` when needed instead of guessing their contents.\n\n",
    );
    for skill in matched {
        block.push_str(&format!("### Skill: {}\n\n", skill.name));
        if !skill.description.is_empty() {
            block.push_str(&format!("_{}_\n\n", skill.description));
        }
        let workspace = workspace_status_for_skill(vault, &skill);
        block.push_str(&format!("Workspace path: `{}`\n", workspace.workspace_root));
        if skill.workspace_manifest().is_some() && !workspace.workspace_ready {
            block.push_str(
                "Workspace is not fully prepared yet. Call `skills_prepare_workspace` before creating declared folders or template documents.\n\n",
            );
        } else {
            block.push_str("Any derived documents should be written into that workspace.\n\n");
        }
        if !skill.allowed_tools.is_empty() {
            block.push_str(&format!(
                "Requested tools: {}\n\n",
                skill.allowed_tools.join(", ")
            ));
        }
        block.push_str(&skill_prompt_body(&skill.content));
        block.push_str("\n\n---\n\n");
    }
    block
}

fn skill_prompt_body(content: &str) -> String {
    let char_count = content.chars().count();
    if char_count <= MAX_SKILL_PROMPT_BODY_CHARS {
        return content.to_string();
    }

    let truncated: String = content.chars().take(MAX_SKILL_PROMPT_BODY_CHARS).collect();
    format!(
        "{truncated}\n\n[skill content truncated: {char_count} chars total, showing first {MAX_SKILL_PROMPT_BODY_CHARS}]"
    )
}
