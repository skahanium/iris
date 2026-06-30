use crate::ai_runtime::ToolAccessLevel;

use super::{ToolCatalogEntry, ToolImplementationStatus};

pub(super) fn tools() -> Vec<ToolCatalogEntry> {
    vec![ToolCatalogEntry {
        name: "skills_list",
        description: "List confirmed prompt-only Iris Skills for global and current vault scopes.",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {}
        }),
        access_level: ToolAccessLevel::ReadIndex,
        requires_confirmation: false,
        implementation: ToolImplementationStatus::Dispatchable,
        default_enabled_without_skill: true,
        scene_affinity: &[],
        max_results: None,
    }]
}
