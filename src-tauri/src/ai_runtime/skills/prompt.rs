use crate::ai_runtime::AiScene;

use super::{skills_for_scene, SkillEntry};

const MAX_SKILL_PROMPT_BODY_CHARS: usize = 12_000;

/// Build system prompt fragment from enabled skills.
pub fn inject_into_prompt(skills: &[SkillEntry], scene: AiScene, user_message: &str) -> String {
    let matched = skills_for_scene(skills, scene, user_message);
    if matched.is_empty() {
        return String::new();
    }
    let mut block = String::from("## 已激活 Skills\n\n");
    block.push_str(
        "若 SKILL 正文引用 `references/`、`resources/` 或 `assets/` 下的文件，\
         请调用 `skills_read_resource` 按需读取，不要猜测内容。\n\n",
    );
    for skill in matched {
        block.push_str(&format!("### Skill: {}\n\n", skill.name));
        if !skill.description.is_empty() {
            block.push_str(&format!("_{}_\n\n", skill.description));
        }
        if !skill.allowed_tools.is_empty() {
            block.push_str(&format!("请求工具: {}\n\n", skill.allowed_tools.join(", ")));
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
