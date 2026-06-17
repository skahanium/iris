//! Unified assistant intent routing (frontend ↔ backend contract).

use serde::{Deserialize, Serialize};

use crate::ai_types::{AgentIntent, AiScene, OrganizeTaskType};
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
            AssistantIntent::Writing | AssistantIntent::Chapter | AssistantIntent::Document => {
                AiScene::DraftingAssist
            }
            AssistantIntent::Citation | AssistantIntent::Research => AiScene::ResearchSynthesis,
            AssistantIntent::Chat | AssistantIntent::Knowledge | AssistantIntent::Organize => {
                AiScene::KnowledgeLookup
            }
        }
    }
}

impl AgentIntent {
    /// Return the internal compatibility scene used for profiles and summaries.
    pub fn scene(self) -> AiScene {
        match self {
            AgentIntent::RewriteSelection
            | AgentIntent::Write
            | AgentIntent::Chapter
            | AgentIntent::DocumentCheck => AiScene::DraftingAssist,
            AgentIntent::Research | AgentIntent::CitationCheck => AiScene::ResearchSynthesis,
            AgentIntent::Chat
            | AgentIntent::AskNotes
            | AgentIntent::Organize
            | AgentIntent::VisionChat
            | AgentIntent::SkillManagement => AiScene::KnowledgeLookup,
        }
    }
}

/// Map a legacy assistant intent into the Phase 2 agent intent contract.
pub fn agent_intent_from_legacy(
    legacy: AssistantIntent,
    has_selection: Option<bool>,
) -> AgentIntent {
    match legacy {
        AssistantIntent::Chat => AgentIntent::Chat,
        AssistantIntent::Knowledge => AgentIntent::AskNotes,
        AssistantIntent::Writing => {
            if has_selection.unwrap_or(true) {
                AgentIntent::RewriteSelection
            } else {
                AgentIntent::Write
            }
        }
        AssistantIntent::Citation => AgentIntent::CitationCheck,
        AssistantIntent::Organize => AgentIntent::Organize,
        AssistantIntent::Research => AgentIntent::Research,
        AssistantIntent::Chapter => AgentIntent::Chapter,
        AssistantIntent::Document => AgentIntent::DocumentCheck,
    }
}

/// Map a Phase 2 agent intent back to the existing workflow intent.
pub fn legacy_intent_for_agent(agent: AgentIntent) -> AssistantIntent {
    match agent {
        AgentIntent::AskNotes => AssistantIntent::Knowledge,
        AgentIntent::RewriteSelection | AgentIntent::Write => AssistantIntent::Writing,
        AgentIntent::CitationCheck => AssistantIntent::Citation,
        AgentIntent::Research => AssistantIntent::Research,
        AgentIntent::Organize => AssistantIntent::Organize,
        AgentIntent::Chapter => AssistantIntent::Chapter,
        AgentIntent::DocumentCheck => AssistantIntent::Document,
        AgentIntent::VisionChat | AgentIntent::SkillManagement | AgentIntent::Chat => {
            AssistantIntent::Chat
        }
    }
}

/// Parse organize task type string from the assistant request.
pub fn parse_organize_task_type(raw: Option<&str>) -> OrganizeTaskType {
    match raw.unwrap_or("full_audit").trim() {
        "title_suggestions" => OrganizeTaskType::TitleSuggestions,
        "tag_suggestions" => OrganizeTaskType::TagSuggestions,
        "folder_suggestions" => OrganizeTaskType::FolderSuggestions,
        "link_suggestions" => OrganizeTaskType::LinkSuggestions,
        _ => OrganizeTaskType::FullAudit,
    }
}

/// Parse document check type string from the assistant request.
pub fn parse_document_check_type(raw: Option<&str>) -> DocumentCheckType {
    match raw.unwrap_or("outline_check").trim() {
        "citation_gap_check" => DocumentCheckType::CitationGapCheck,
        "style_consistency" => DocumentCheckType::StyleConsistency,
        "cross_doc_reference" => DocumentCheckType::CrossDocReference,
        _ => DocumentCheckType::OutlineCheck,
    }
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
    fn agent_intent_scene_mapping() {
        assert_eq!(AgentIntent::Research.scene(), AiScene::ResearchSynthesis);
        assert_eq!(
            AgentIntent::RewriteSelection.scene(),
            AiScene::DraftingAssist
        );
        assert_eq!(AgentIntent::Write.scene(), AiScene::DraftingAssist);
        assert_eq!(
            AgentIntent::CitationCheck.scene(),
            AiScene::ResearchSynthesis
        );
        assert_eq!(AgentIntent::AskNotes.scene(), AiScene::KnowledgeLookup);
        assert_eq!(
            AgentIntent::SkillManagement.scene(),
            AiScene::KnowledgeLookup
        );
    }

    #[test]
    fn legacy_intent_maps_to_phase2_agent_intent() {
        assert_eq!(
            agent_intent_from_legacy(AssistantIntent::Knowledge, None),
            AgentIntent::AskNotes
        );
        assert_eq!(
            agent_intent_from_legacy(AssistantIntent::Writing, Some(true)),
            AgentIntent::RewriteSelection
        );
        assert_eq!(
            agent_intent_from_legacy(AssistantIntent::Writing, Some(false)),
            AgentIntent::Write
        );
        assert_eq!(
            agent_intent_from_legacy(AssistantIntent::Citation, None),
            AgentIntent::CitationCheck
        );
        assert_eq!(
            agent_intent_from_legacy(AssistantIntent::Document, None),
            AgentIntent::DocumentCheck
        );
    }

    #[test]
    fn parses_organize_task_type() {
        assert_eq!(
            parse_organize_task_type(Some("tag_suggestions")),
            OrganizeTaskType::TagSuggestions
        );
        assert_eq!(
            parse_organize_task_type(Some("\"tag_suggestions\"")),
            OrganizeTaskType::FullAudit
        );
    }

    #[test]
    fn parses_document_check_type() {
        assert_eq!(
            parse_document_check_type(Some("style_consistency")),
            DocumentCheckType::StyleConsistency
        );
        assert_eq!(
            parse_document_check_type(Some("\"style_consistency\"")),
            DocumentCheckType::OutlineCheck
        );
    }
}
