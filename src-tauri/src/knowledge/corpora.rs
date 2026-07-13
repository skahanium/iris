//! Vault corpus definitions (`.iris/corpora.toml`) for intent-scoped retrieval.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ai_types::{AgentIntent, CorpusPacketMeta};
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
    pub intents: Vec<String>,
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
    let config: CorpusConfig =
        toml::from_str(&raw).map_err(|e| AppError::msg(format!("Invalid corpora.toml: {e}")))?;
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

/// Path prefixes bound to a task intent via `intents = [...]`.
pub fn prefixes_for_intent(config: &CorpusConfig, intent: AgentIntent) -> Vec<String> {
    let intent_key = serde_json::to_string(&intent).unwrap_or_default();
    let intent_key = intent_key.trim_matches('"');
    config
        .corpus
        .iter()
        .filter(|corpus| corpus.intents.iter().any(|value| value == intent_key))
        .map(|corpus| normalize_prefix(&corpus.path_prefix))
        .filter(|prefix| !prefix.is_empty())
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

/// Canonical corpus role, keeping legacy values readable.
pub fn canonical_kind(kind: &str) -> &'static str {
    match kind.trim() {
        "authority" | "regulation" => "authority",
        "exemplar" => "exemplar",
        "reference" => "reference",
        "lookup" | "general" => "lookup",
        _ => "authority",
    }
}

/// User-facing corpus role label.
pub fn corpus_role_label(kind: &str) -> &'static str {
    match canonical_kind(kind) {
        "authority" => "规范依据",
        "exemplar" => "范文样本",
        "reference" => "参考资料",
        "lookup" => "查阅资料",
        _ => "规范依据",
    }
}

/// Prompt instruction for how the assistant may use this corpus role.
pub fn corpus_role_instruction(kind: &str) -> &'static str {
    match canonical_kind(kind) {
        "authority" => "必须优先遵循，可作为结论依据。",
        "exemplar" => "只学习结构、语气和表达方式，不采纳其中事实或结论作为依据。",
        "reference" => "可作为背景参考，但不能当作规范遵循。",
        "lookup" => "可摘要其内容，但必须标注仅供查阅，不能作为依据。",
        _ => "必须优先遵循，可作为结论依据。",
    }
}

/// Whether this corpus role may support normative conclusions.
pub fn corpus_role_can_be_authority(kind: &str) -> bool {
    canonical_kind(kind) == "authority"
}

/// Find the most specific corpus entry for a vault-relative path.
pub fn corpus_for_path<'a>(config: &'a CorpusConfig, path: &str) -> Option<&'a CorpusEntry> {
    let norm = path.replace('\\', "/");
    config
        .corpus
        .iter()
        .filter_map(|entry| {
            let prefix = normalize_prefix(&entry.path_prefix);
            if prefix.is_empty() || !norm.starts_with(prefix.as_str()) {
                return None;
            }
            Some((prefix.len(), entry))
        })
        .max_by_key(|(len, _)| *len)
        .map(|(_, entry)| entry)
}

/// Build safe prompt metadata for a corpus entry.
pub fn packet_meta_for_entry(entry: &CorpusEntry) -> CorpusPacketMeta {
    let kind = canonical_kind(&entry.kind).to_string();
    CorpusPacketMeta {
        id: entry.id.clone(),
        name: entry.name.clone(),
        label: corpus_role_label(&kind).to_string(),
        instruction: corpus_role_instruction(&kind).to_string(),
        can_be_authority: corpus_role_can_be_authority(&kind),
        kind,
    }
}

/// Whether a file path falls under any authority corpus prefix.
pub fn is_regulation_corpus_path(config: &CorpusConfig, path: &str) -> bool {
    let norm = path.replace('\\', "/");
    config.corpus.iter().any(|c| {
        canonical_kind(&c.kind) == "authority"
            && norm.starts_with(normalize_prefix(&c.path_prefix).as_str())
    })
}

/// If no authority corpus is configured, index regulation structure for all notes (legacy).
pub fn should_index_regulation_for_path(config: &CorpusConfig, path: &str) -> bool {
    let has_regulation_corpus = config
        .corpus
        .iter()
        .any(|c| canonical_kind(&c.kind) == "authority");
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
intents = ["ask_notes"]
"#;
        let config: CorpusConfig = toml::from_str(raw).unwrap();
        assert_eq!(config.corpus.len(), 1);
        assert_eq!(config.corpus[0].id, "party_discipline");
    }

    #[test]
    fn prefixes_for_intent_filters() {
        let config = CorpusConfig {
            corpus: vec![CorpusEntry {
                id: "a".into(),
                name: "A".into(),
                path_prefix: "党纪法规/".into(),
                kind: "regulation".into(),
                intents: vec!["ask_notes".into()],
            }],
        };
        let prefixes = prefixes_for_intent(&config, AgentIntent::AskNotes);
        assert_eq!(prefixes, vec!["党纪法规/"]);
        assert!(prefixes_for_intent(&config, AgentIntent::Write).is_empty());
    }

    #[test]
    fn canonical_kind_maps_legacy_roles() {
        assert_eq!(canonical_kind("regulation"), "authority");
        assert_eq!(canonical_kind("general"), "lookup");
        assert_eq!(canonical_kind("exemplar"), "exemplar");
        assert_eq!(canonical_kind("reference"), "reference");
        assert_eq!(canonical_kind("lookup"), "lookup");
        assert_eq!(canonical_kind("unknown"), "authority");
    }

    #[test]
    fn corpus_for_path_prefers_longest_matching_prefix() {
        let config = CorpusConfig {
            corpus: vec![
                CorpusEntry {
                    id: "root".into(),
                    name: "Root".into(),
                    path_prefix: "materials/".into(),
                    kind: "lookup".into(),
                    intents: vec!["ask_notes".into()],
                },
                CorpusEntry {
                    id: "nested".into(),
                    name: "Nested".into(),
                    path_prefix: "materials/rules/".into(),
                    kind: "authority".into(),
                    intents: vec!["ask_notes".into()],
                },
            ],
        };

        let entry = corpus_for_path(&config, "materials/rules/a.md").unwrap();
        assert_eq!(entry.id, "nested");
        assert_eq!(corpus_role_label(&entry.kind), "规范依据");
    }
}
