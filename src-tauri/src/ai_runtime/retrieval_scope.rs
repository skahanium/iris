//! Retrieval path scope for hybrid search (corpus + @ mentions).

use serde::{Deserialize, Serialize};

use crate::ai_runtime::AiScene;
use crate::knowledge::corpora::{self, CorpusConfig};

/// User-provided scope from IPC (`@` mentions + optional corpus IDs).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextScopeDto {
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub path_prefixes: Vec<String>,
    #[serde(default)]
    pub corpus_ids: Vec<String>,
}

/// Resolved scope used by the retrieval broker.
#[derive(Debug, Clone, Default)]
pub struct RetrievalScope {
    pub path_prefixes: Vec<String>,
    pub paths: Vec<String>,
}

impl RetrievalScope {
    pub fn is_unrestricted(&self) -> bool {
        self.path_prefixes.is_empty() && self.paths.is_empty()
    }

    pub fn matches_path(&self, path: &str) -> bool {
        let norm = path.replace('\\', "/");
        for exact in &self.paths {
            if norm == exact.replace('\\', "/") {
                return true;
            }
        }
        for prefix in &self.path_prefixes {
            if norm.starts_with(prefix.as_str()) {
                return true;
            }
        }
        false
    }

    fn push_prefix(&mut self, prefix: String) {
        let norm = corpora::normalize_prefix(&prefix);
        if norm.is_empty() {
            return;
        }
        if !self.path_prefixes.iter().any(|p| p == &norm) {
            self.path_prefixes.push(norm);
        }
    }

    fn push_path(&mut self, path: String) {
        let norm = path.replace('\\', "/");
        if norm.is_empty() {
            return;
        }
        if !self.paths.iter().any(|p| p == &norm) {
            self.paths.push(norm);
        }
    }
}

/// Resolve retrieval scope.
///
/// User-provided `@` paths, prefixes, or corpus IDs are a hard boundary: when
/// present, scene defaults are not unioned in.
pub fn resolve_retrieval_scope(
    vault_corpora: &CorpusConfig,
    scene: AiScene,
    user: &ContextScopeDto,
) -> RetrievalScope {
    let mut scope = RetrievalScope::default();

    let has_user_scope =
        !user.paths.is_empty() || !user.path_prefixes.is_empty() || !user.corpus_ids.is_empty();

    if has_user_scope {
        for prefix in corpora::prefixes_for_corpus_ids(vault_corpora, &user.corpus_ids) {
            scope.push_prefix(prefix);
        }
        for prefix in &user.path_prefixes {
            scope.push_prefix(prefix.clone());
        }
        for path in &user.paths {
            scope.push_path(path.clone());
        }
    } else {
        for prefix in corpora::prefixes_for_scene(vault_corpora, scene) {
            scope.push_prefix(prefix);
        }
    }

    scope
}

pub fn filter_packets_by_scope<T>(
    packets: &mut Vec<T>,
    scope: &RetrievalScope,
    path_fn: impl Fn(&T) -> Option<&str>,
) {
    if scope.is_unrestricted() {
        return;
    }
    packets.retain(|p| match path_fn(p) {
        None => true,
        Some(path) => scope.matches_path(path),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::corpora::{CorpusConfig, CorpusEntry};

    #[test]
    fn matches_prefix_and_exact() {
        let scope = RetrievalScope {
            path_prefixes: vec!["党纪法规/".into()],
            paths: vec!["范文/样例.md".into()],
        };
        assert!(scope.matches_path("党纪法规/条例.md"));
        assert!(scope.matches_path("范文/样例.md"));
        assert!(!scope.matches_path("其他/笔记.md"));
    }

    fn corpus_config() -> CorpusConfig {
        CorpusConfig {
            corpus: vec![
                CorpusEntry {
                    id: "authority".into(),
                    name: "制度".into(),
                    path_prefix: "制度/".into(),
                    kind: "authority".into(),
                    scenes: vec!["knowledge_lookup".into()],
                },
                CorpusEntry {
                    id: "drafts".into(),
                    name: "草稿".into(),
                    path_prefix: "草稿/".into(),
                    kind: "lookup".into(),
                    scenes: Vec::new(),
                },
            ],
        }
    }

    #[test]
    fn user_scope_is_hard_boundary_over_scene_defaults() {
        let user = ContextScopeDto {
            paths: vec!["草稿/指定.md".into()],
            path_prefixes: vec!["项目/".into()],
            corpus_ids: Vec::new(),
        };

        let scope = resolve_retrieval_scope(&corpus_config(), AiScene::KnowledgeLookup, &user);

        assert!(scope.matches_path("草稿/指定.md"));
        assert!(scope.matches_path("项目/计划.md"));
        assert!(!scope.matches_path("制度/条例.md"));
        assert_eq!(scope.path_prefixes, vec!["项目/"]);
        assert_eq!(scope.paths, vec!["草稿/指定.md"]);
    }

    #[test]
    fn explicit_corpus_ids_are_hard_boundary_over_scene_defaults() {
        let user = ContextScopeDto {
            paths: Vec::new(),
            path_prefixes: Vec::new(),
            corpus_ids: vec!["drafts".into()],
        };

        let scope = resolve_retrieval_scope(&corpus_config(), AiScene::KnowledgeLookup, &user);

        assert!(scope.matches_path("草稿/备忘.md"));
        assert!(!scope.matches_path("制度/条例.md"));
        assert_eq!(scope.path_prefixes, vec!["草稿/"]);
    }
}
