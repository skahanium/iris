//! Unified assistant intent routing (frontend ↔ backend contract).

use serde::{Deserialize, Serialize};

use crate::ai_types::{AiScene, OrganizeTaskType};
use crate::ai_workflows::document_workflow::DocumentCheckType;

/// Mirrors frontend `AssistantIntent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantIntent {
    Chat,
    Knowledge,
    Writing,
    Citation,
    Organize,
    Research,
    Chapter,
    Document,
}

impl AssistantIntent {
    pub fn scene(self) -> AiScene {
        match self {
            AssistantIntent::Writing
            | AssistantIntent::Citation
            | AssistantIntent::Chapter
            | AssistantIntent::Document => AiScene::DraftingAssist,
            AssistantIntent::Research => AiScene::ResearchSynthesis,
            AssistantIntent::Chat | AssistantIntent::Knowledge | AssistantIntent::Organize => {
                AiScene::KnowledgeLookup
            }
        }
    }
}

/// Parse organize task type string from the assistant request.
pub fn parse_organize_task_type(raw: Option<&str>) -> OrganizeTaskType {
    let s = raw.unwrap_or("full_audit");
    serde_json::from_str(&format!("\"{s}\"")).unwrap_or(OrganizeTaskType::FullAudit)
}

/// Parse document check type string from the assistant request.
pub fn parse_document_check_type(raw: Option<&str>) -> DocumentCheckType {
    let s = raw.unwrap_or("outline_check");
    serde_json::from_str(&format!("\"{s}\"")).unwrap_or(DocumentCheckType::OutlineCheck)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_scene_mapping() {
        assert_eq!(
            AssistantIntent::Research.scene(),
            AiScene::ResearchSynthesis
        );
        assert_eq!(AssistantIntent::Writing.scene(), AiScene::DraftingAssist);
        assert_eq!(AssistantIntent::Knowledge.scene(), AiScene::KnowledgeLookup);
    }

    #[test]
    fn parses_organize_task_type() {
        assert_eq!(
            parse_organize_task_type(Some("tag_suggestions")),
            OrganizeTaskType::TagSuggestions
        );
    }
}
