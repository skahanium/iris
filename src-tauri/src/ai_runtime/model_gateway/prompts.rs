use crate::ai_types::{AiScene, ContextPacket};

/// Build the context-aware prompt for drafting scenes.
///
/// Combines document outline, cursor context, evidence packets, and writing rules.
pub fn build_drafting_prompt(
    document_outline: &str,
    cursor_context: &str,
    packets: &[ContextPacket],
    user_rules: &[String],
) -> String {
    let mut prompt = String::new();

    prompt.push_str("## 当前文稿大纲\n\n");
    prompt.push_str(document_outline);
    prompt.push_str("\n\n## 光标邻域上下文\n\n");
    prompt.push_str(cursor_context);

    if !packets.is_empty() {
        prompt.push_str("\n\n## 参考材料\n\n");
        for packet in packets {
            prompt.push_str(&format!("- [{}] {}\n", packet.citation_label, packet.title));
            prompt.push_str(&format!("  {}\n", packet.excerpt));
        }
    }

    if !user_rules.is_empty() {
        prompt.push_str("\n\n## 写作规则\n\n");
        for rule in user_rules {
            prompt.push_str(&format!("- {}\n", rule));
        }
    }

    prompt
}

/// Build a citation recommendation prompt for a paragraph and candidate packets.
pub fn build_citation_prompt(paragraph: &str, candidates: &[ContextPacket]) -> String {
    let mut prompt = String::new();

    prompt.push_str("分析以下段落，推荐合适的法规引用：\n\n");
    prompt.push_str(paragraph);
    prompt.push_str("\n\n可选的引用来源：\n\n");

    for candidate in candidates {
        prompt.push_str(&format!(
            "[{}] {} - {}\n",
            candidate.citation_label, candidate.title, candidate.excerpt
        ));
    }

    prompt.push_str("\n请推荐最相关的引用，并说明理由。");
    prompt
}

/// Determine whether a user profile rule applies to a given AI scene.
///
/// Scoped rules (writing_style, citation_habits) only apply to relevant scenes.
/// Global rules (custom_rules, tool_preferences, etc.) apply everywhere.
pub(super) fn is_rule_applicable_for_scene(key: &str, scene: AiScene) -> bool {
    match key {
        "writing_style" => {
            matches!(scene, AiScene::DraftingAssist | AiScene::ExemplarLearning)
        }
        "citation_habits" => {
            matches!(
                scene,
                AiScene::DraftingAssist | AiScene::ResearchSynthesis | AiScene::KnowledgeLookup
            )
        }
        "tool_preferences" | "model_preferences" | "custom_rules" | "agent_behavior" => true,
        _ => !matches!(scene, AiScene::KnowledgeLookup),
    }
}
