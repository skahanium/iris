use std::path::Path;

use crate::ai_types::AgentIntent;

use super::{skills_for_task, SkillEntry};

const MAX_SKILL_PROMPT_BODY_CHARS: usize = 12_000;

/// Build system prompt fragment from enabled skills.
pub fn inject_into_prompt(
    vault: &Path,
    skills: &[SkillEntry],
    intent: AgentIntent,
    user_message: &str,
) -> String {
    let matched = skills_for_task(skills, intent, user_message, &[], None);
    if matched.is_empty() {
        return String::new();
    }
    let mut block = String::from("## Activated Skills\n\n");
    block.push_str(
        "Skills are prompt-only instructions confirmed by the user. Use only the activated instruction text below; do not install external packages, registries, CLI tools, or additional skill resources during a run.\n\n",
    );
    for skill in matched {
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
