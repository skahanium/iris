//! Scene router: maps scene to workflow profile and context strategy.
//!
//! The canonical `SceneProfile` struct and `resolve_scene` function now live
//! in `crate::ai_types`. This module re-exports them for backward compatibility.

pub use crate::ai_types::{resolve_scene, SceneProfile};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_types::AiScene;

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
