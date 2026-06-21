use std::sync::Arc;

use crate::ai_runtime::{AiScene, ContextPacket};
use crate::app::AppState;

pub struct ToolDispatchContext<'a> {
    pub scene: AiScene,
    pub note_path: Option<&'a str>,
    pub file_id: Option<i64>,
    pub web_search_enabled: bool,
    pub cold_start_packets: &'a [ContextPacket],
    pub app_handle: Option<tauri::AppHandle>,
    pub attachment_count: usize,
    pub skill_activation_plan: Option<&'a crate::ai_types::SkillActivationPlanSummary>,
    pub embedding_state: Option<&'a Arc<AppState>>,
}

impl<'a> ToolDispatchContext<'a> {
    pub(crate) fn index_embedding_mode(&self) -> crate::indexer::scan::IndexEmbeddingMode<'_> {
        self.embedding_state
            .map(crate::indexer::scan::IndexEmbeddingMode::Queue)
            .unwrap_or(crate::indexer::scan::IndexEmbeddingMode::Skip)
    }
}
