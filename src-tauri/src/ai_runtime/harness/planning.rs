//! Harness round budget and agent-loop planning limits.

use crate::ai_runtime::scene_router::resolve_scene;
use crate::ai_runtime::AiScene;

/// Resolve effective max agentic rounds (respects override and scene profile cap).
pub(crate) fn resolve_max_rounds(scene: AiScene, max_rounds_override: Option<u32>) -> u32 {
    let profile = resolve_scene(scene);
    max_rounds_override
        .unwrap_or(profile.max_agentic_rounds)
        .min(profile.max_agentic_rounds)
}

/// Token budget for a harness run (defaults to scene profile).
pub(crate) fn resolve_token_budget(scene: AiScene, token_budget: Option<u32>) -> u32 {
    let profile = resolve_scene(scene);
    token_budget.unwrap_or(profile.max_token_budget as u32)
}
