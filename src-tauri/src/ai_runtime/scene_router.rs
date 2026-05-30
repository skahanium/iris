//! Scene router: maps scene to workflow profile and context strategy.
//!
//! Phase A: infrastructure only — returns profile metadata.
//! Phase B+: wires in retrieval strategies per scene.

use crate::ai_runtime::AiScene;

/// Scene profile: describes what capabilities a scene activates.
#[derive(Debug, Clone)]
pub struct SceneProfile {
    pub scene: AiScene,
    pub autonomy_level: crate::ai_runtime::AutonomyLevel,
    pub default_global_scope: bool,
    pub max_agentic_rounds: u32,
    pub max_tool_calls_per_round: u32,
    pub default_token_budget: usize,
    pub max_token_budget: usize,
}

/// Resolve a scene to its profile.
pub fn resolve_scene(scene: AiScene) -> SceneProfile {
    match scene {
        AiScene::KnowledgeLookup => SceneProfile {
            scene,
            autonomy_level: crate::ai_runtime::AutonomyLevel::L2,
            default_global_scope: true,
            max_agentic_rounds: 3,
            max_tool_calls_per_round: 4,
            default_token_budget: 6_000,
            max_token_budget: 12_000,
        },
        AiScene::ExemplarLearning => SceneProfile {
            scene,
            autonomy_level: crate::ai_runtime::AutonomyLevel::L2,
            default_global_scope: false,
            max_agentic_rounds: 2,
            max_tool_calls_per_round: 4,
            default_token_budget: 10_000,
            max_token_budget: 20_000,
        },
        AiScene::DraftingAssist => SceneProfile {
            scene,
            autonomy_level: crate::ai_runtime::AutonomyLevel::L2,
            default_global_scope: false,
            max_agentic_rounds: 3,
            max_tool_calls_per_round: 5,
            default_token_budget: 12_000,
            max_token_budget: 25_000,
        },
        AiScene::ResearchSynthesis => SceneProfile {
            scene,
            autonomy_level: crate::ai_runtime::AutonomyLevel::L3,
            default_global_scope: true,
            max_agentic_rounds: 4,
            max_tool_calls_per_round: 6,
            default_token_budget: 20_000,
            max_token_budget: 50_000,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l3_scene_allows_agentic_loop() {
        let profile = resolve_scene(AiScene::ResearchSynthesis);
        assert_eq!(profile.max_agentic_rounds, 4);
        assert_eq!(profile.max_tool_calls_per_round, 6);
    }

    #[test]
    fn knowledge_scene_allows_multi_round() {
        let profile = resolve_scene(AiScene::KnowledgeLookup);
        assert!(profile.max_agentic_rounds >= 2);
    }
}
