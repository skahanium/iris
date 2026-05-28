//! Vault corpus definitions (`.iris/corpora.toml`) for scene-scoped retrieval.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ai_runtime::AiScene;
use crate::error::{AppError, AppResult};

/// Relative path inside the vault for corpus configuration.
pub const CORPORA_REL_PATH: &str = ".iris/corpora.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CorpusConfig {
    #[serde(default)]
    pub corpus: Vec<CorpusEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorpusEntry {
    pub id: String,
    pub name: String,
    pub path_prefix: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub scenes: Vec<String>,
}

fn default_kind() -> String {
    "general".to_string()
}

/// Load corpora from vault; missing file yields empty config.
pub fn load_corpora(vault: &Path) -> AppResult<CorpusConfig> {
    let path = vault.join(CORPORA_REL_PATH);
    if !path.is_file() {
        return Ok(CorpusConfig::default());
    }
    let raw = std::fs::read_to_string(&path)?;
    let config: CorpusConfig = toml::from_str(&raw)
        .map_err(|e| AppError::msg(format!("Invalid corpora.toml: {e}")))?;
    Ok(config)
}

/// Persist corpus configuration to vault.
pub fn save_corpora(vault: &Path, config: &CorpusConfig) -> AppResult<()> {
    let path = vault.join(CORPORA_REL_PATH);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = toml::to_string_pretty(config)
        .map_err(|e| AppError::msg(format!("Failed to serialize corpora.toml: {e}")))?;
    std::fs::write(path, raw)?;
    Ok(())
}

/// Normalize folder prefix for `path.starts_with` checks.
pub fn normalize_prefix(prefix: &str) -> String {
    let trimmed = prefix.trim().replace('\\', "/");
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.ends_with('/') {
        trimmed
    } else {
        format!("{trimmed}/")
    }
}

/// Path prefixes bound to a scene via `scenes = [...]`.
pub fn prefixes_for_scene(config: &CorpusConfig, scene: AiScene) -> Vec<String> {
    let scene_key = scene.profile();
    config
        .corpus
        .iter()
        .filter(|c| c.scenes.iter().any(|s| s == scene_key))
        .map(|c| normalize_prefix(&c.path_prefix))
        .filter(|p| !p.is_empty())
        .collect()
}

/// Resolve corpus IDs to path prefixes.
pub fn prefixes_for_corpus_ids(config: &CorpusConfig, ids: &[String]) -> Vec<String> {
    ids.iter()
        .filter_map(|id| {
            config
                .corpus
                .iter()
                .find(|c| c.id == *id)
                .map(|c| normalize_prefix(&c.path_prefix))
        })
        .filter(|p| !p.is_empty())
        .collect()
}

/// Whether a file path falls under any regulation-kind corpus prefix.
pub fn is_regulation_corpus_path(config: &CorpusConfig, path: &str) -> bool {
    let norm = path.replace('\\', "/");
    config.corpus.iter().any(|c| {
        c.kind == "regulation" && norm.starts_with(normalize_prefix(&c.path_prefix).as_str())
    })
}

/// If no regulation corpus is configured, index regulation structure for all notes (legacy).
pub fn should_index_regulation_for_path(config: &CorpusConfig, path: &str) -> bool {
    let has_regulation_corpus = config
        .corpus
        .iter()
        .any(|c| c.kind == "regulation");
    if !has_regulation_corpus {
        return true;
    }
    is_regulation_corpus_path(config, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_example_toml() {
        let raw = r#"
[[corpus]]
id = "party_discipline"
name = "党纪法规"
path_prefix = "党纪法规/"
kind = "regulation"
scenes = ["knowledge_lookup"]
"#;
        let config: CorpusConfig = toml::from_str(raw).unwrap();
        assert_eq!(config.corpus.len(), 1);
        assert_eq!(config.corpus[0].id, "party_discipline");
    }

    #[test]
    fn prefixes_for_scene_filters() {
        let config = CorpusConfig {
            corpus: vec![CorpusEntry {
                id: "a".into(),
                name: "A".into(),
                path_prefix: "党纪法规/".into(),
                kind: "regulation".into(),
                scenes: vec!["knowledge_lookup".into()],
            }],
        };
        let prefixes = prefixes_for_scene(&config, AiScene::KnowledgeLookup);
        assert_eq!(prefixes, vec!["党纪法规/"]);
        assert!(prefixes_for_scene(&config, AiScene::DraftingAssist).is_empty());
    }
}
