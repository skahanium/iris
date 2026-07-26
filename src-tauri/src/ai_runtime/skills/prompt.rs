use std::path::Path;

use crate::ai_types::AgentIntent;

use super::{skills_for_task, SkillConfirmationStatus, SkillEntry};

/// A Run may inject at most a primary and an auxiliary Skill. Keeping each
/// body below half the aggregate cap makes the combined untrusted instruction
/// payload bounded even before a provider-specific tokenizer is selected.
const MAX_SKILL_PROMPT_BODY_CHARS: usize = 4_000;

/// Build system prompt fragment from enabled skills.
pub fn inject_into_prompt(
    vault: &Path,
    skills: &[SkillEntry],
    intent: AgentIntent,
    user_message: &str,
) -> String {
    let matched = skills_for_task(skills, intent, user_message, &[], None);
    inject_selected_skills_into_prompt(vault, &matched)
}

/// Build a prompt overlay from the exact skills resolved by an activation plan.
///
/// This is intentionally I/O-free: selection happens against the cached activation index before
/// the run, and this function only renders the already-loaded primary and auxiliary skills.
pub fn inject_selected_skills_into_prompt(vault: &Path, skills: &[SkillEntry]) -> String {
    let selected: Vec<_> = skills
        .iter()
        .filter(|skill| {
            skill.enabled && skill.confirmation_status == SkillConfirmationStatus::Confirmed
        })
        .take(2)
        .collect();
    if selected.is_empty() {
        return String::new();
    }
    let mut block = String::from("## Activated Skills\n\n");
    block.push_str(
        "Skills are prompt-only instructions confirmed by the user. Use only the activated instruction text below; do not install external packages, registries, CLI tools, or additional skill resources during a run.\n\n",
    );
    for skill in selected {
        block.push_str(&format!("### Skill: {}\n\n", skill.name));
        if !skill.description.is_empty() {
            block.push_str(&format!("_{}_\n\n", skill.description));
        }
        let _ = vault;
        block.push_str(
            "Write ordinary note changes only through the normal user-confirmed note editing flow.\n\n",
        );
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
