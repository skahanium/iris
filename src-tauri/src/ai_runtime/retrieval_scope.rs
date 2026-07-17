//! Retrieval path scope for hybrid search (corpus + @ mentions).

use std::collections::HashSet;
use std::path::{Component, Path};

use rusqlite::{params_from_iter, Connection};
use serde::{Deserialize, Serialize};

use crate::knowledge::corpora::{self, CorpusConfig};
use crate::{error::AppError, error::AppResult};

/// User-provided scope from IPC (`@` mentions + optional corpus IDs).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextScopeDto {
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub path_prefixes: Vec<String>,
    #[serde(default)]
    pub corpus_ids: Vec<String>,
    #[serde(default)]
    pub required_tags: Vec<String>,
}

/// Resolved scope used by the retrieval broker.
#[derive(Debug, Clone, Default)]
pub struct RetrievalScope {
    pub path_prefixes: Vec<String>,
    pub paths: Vec<String>,
    pub required_tags: Vec<String>,
}

impl RetrievalScope {
    pub fn is_unrestricted(&self) -> bool {
        self.is_path_unrestricted() && self.required_tags.is_empty()
    }

    pub fn is_path_unrestricted(&self) -> bool {
        self.path_prefixes.is_empty() && self.paths.is_empty()
    }

    pub fn matches_path(&self, path: &str) -> bool {
        let Ok(norm) = normalize_relative(path, false) else {
            return false;
        };
        if self.is_path_unrestricted() {
            return true;
        }
        for exact in &self.paths {
            if norm == *exact {
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

    /// Return whether one indexed normal-domain note is inside every path and tag boundary.
    pub(crate) fn allows_path(&self, conn: &Connection, path: &str) -> AppResult<bool> {
        let path = normalize_relative(path, false)?;
        if !self.matches_path(&path) {
            return Ok(false);
        }
        if self.required_tags.is_empty() {
            return Ok(true);
        }
        let placeholders = vec!["?"; self.required_tags.len()].join(",");
        let sql = format!(
            "SELECT COUNT(DISTINCT lower(t.name))
             FROM files f
             JOIN file_tags ft ON ft.file_id = f.id
             JOIN tags t ON t.id = ft.tag_id
             WHERE f.path = ?1 AND lower(t.name) IN ({placeholders})"
        );
        let mut values = Vec::with_capacity(self.required_tags.len() + 1);
        values.push(path);
        values.extend(self.required_tags.iter().cloned());
        let matched: usize =
            conn.query_row(&sql, params_from_iter(values.iter()), |row| row.get(0))?;
        Ok(matched == self.required_tags.len())
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

    fn push_required_tag(&mut self, tag: String) {
        let norm = tag.trim().to_lowercase();
        if norm.is_empty() {
            return;
        }
        if !self.required_tags.iter().any(|item| item == &norm) {
            self.required_tags.push(norm);
        }
    }
}

/// Canonicalize and validate one normal-domain retrieval boundary before it is persisted.
pub(crate) fn normalize_context_scope(input: &ContextScopeDto) -> AppResult<ContextScopeDto> {
    Ok(ContextScopeDto {
        paths: normalize_unique(&input.paths, |value| normalize_relative(value, false))?,
        path_prefixes: normalize_unique(&input.path_prefixes, |value| {
            normalize_relative(value, true)
        })?,
        corpus_ids: normalize_unique(&input.corpus_ids, normalize_identifier)?,
        required_tags: normalize_unique(&input.required_tags, normalize_tag)?,
    })
}

/// Canonicalize one explicit normal-domain note path using the same boundary rules as retrieval.
pub(crate) fn normalize_note_path(value: &str) -> AppResult<String> {
    normalize_relative(value, false)
}

fn normalize_unique(
    values: &[String],
    normalize: impl Fn(&str) -> AppResult<String>,
) -> AppResult<Vec<String>> {
    let mut normalized = Vec::with_capacity(values.len());
    for value in values {
        let value = normalize(value)?;
        if !normalized.iter().any(|item| item == &value) {
            normalized.push(value);
        }
    }
    Ok(normalized)
}

fn normalize_relative(value: &str, prefix: bool) -> AppResult<String> {
    let value = value.trim().replace('\\', "/");
    if value.is_empty()
        || value.starts_with('/')
        || value.chars().any(char::is_control)
        || value
            .split('/')
            .next()
            .is_some_and(|segment| segment.ends_with(':'))
    {
        return Err(AppError::msg("agent_run_invalid_retrieval_scope"));
    }
    let mut parts = Vec::new();
    for component in Path::new(&value).components() {
        match component {
            Component::Normal(part) => {
                let part = part
                    .to_str()
                    .filter(|part| !part.is_empty())
                    .ok_or_else(|| AppError::msg("agent_run_invalid_retrieval_scope"))?;
                parts.push(part);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::msg("agent_run_invalid_retrieval_scope"));
            }
        }
    }
    let normalized = parts.join("/");
    if normalized.is_empty() || crate::storage::paths::has_reserved_path_root(&normalized) {
        return Err(AppError::msg("agent_run_invalid_retrieval_scope"));
    }
    Ok(if prefix {
        corpora::normalize_prefix(&normalized)
    } else {
        normalized
    })
}

fn normalize_identifier(value: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.chars().any(char::is_control)
        || value.contains(['/', '\\'])
    {
        return Err(AppError::msg("agent_run_invalid_retrieval_scope"));
    }
    Ok(value.to_string())
}

fn normalize_tag(value: &str) -> AppResult<String> {
    let value = value.trim().trim_start_matches('#').trim().to_lowercase();
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(AppError::msg("agent_run_invalid_retrieval_scope"));
    }
    Ok(value)
}

/// Resolve retrieval scope.
///
/// User-provided `@` paths, prefixes, or corpus IDs are a hard boundary: when
/// present, intent defaults are not unioned in.
pub fn resolve_retrieval_scope(
    vault_corpora: &CorpusConfig,
    intent: crate::ai_types::AgentIntent,
    user: &ContextScopeDto,
) -> AppResult<RetrievalScope> {
    let user = normalize_context_scope(user)?;
    let mut scope = RetrievalScope::default();

    let has_user_scope = !user.paths.is_empty()
        || !user.path_prefixes.is_empty()
        || !user.corpus_ids.is_empty()
        || !user.required_tags.is_empty();

    if has_user_scope {
        for corpus_id in &user.corpus_ids {
            let corpus = vault_corpora
                .corpus
                .iter()
                .find(|corpus| corpus.id == *corpus_id)
                .ok_or_else(|| AppError::msg("agent_run_invalid_retrieval_scope"))?;
            scope.push_prefix(normalize_relative(&corpus.path_prefix, true)?);
        }
        for prefix in &user.path_prefixes {
            scope.push_prefix(prefix.clone());
        }
        for path in &user.paths {
            scope.push_path(path.clone());
        }
        for tag in &user.required_tags {
            scope.push_required_tag(tag.clone());
        }
    } else {
        for prefix in corpora::prefixes_for_intent(vault_corpora, intent) {
            scope.push_prefix(prefix);
        }
    }

    Ok(scope)
}

pub fn filter_packets_by_scope<T>(
    packets: &mut Vec<T>,
    scope: &RetrievalScope,
    path_fn: impl Fn(&T) -> Option<&str>,
) {
    packets.retain(|p| match path_fn(p) {
        None => scope.is_path_unrestricted(),
        Some(path) => scope.matches_path(path),
    });
}

/// Retain only packets whose file carries every required tag in the scope.
pub fn filter_packets_by_required_tags<T>(
    conn: &Connection,
    packets: &mut Vec<T>,
    scope: &RetrievalScope,
    path_fn: impl Fn(&T) -> Option<&str>,
) -> crate::error::AppResult<()> {
    let mut tags: Vec<String> = scope
        .required_tags
        .iter()
        .map(|tag| tag.trim().to_lowercase())
        .filter(|tag| !tag.is_empty())
        .collect();
    tags.sort();
    tags.dedup();
    if tags.is_empty() {
        return Ok(());
    }

    let placeholders = vec!["?"; tags.len()].join(",");
    let sql = format!(
        "SELECT f.path
         FROM files AS f
         INNER JOIN file_tags AS ft ON ft.file_id = f.id
         INNER JOIN tags AS t ON t.id = ft.tag_id
         WHERE lower(t.name) IN ({placeholders})
         GROUP BY f.id, f.path
         HAVING COUNT(DISTINCT lower(t.name)) = {}",
        tags.len()
    );
    let mut statement = conn.prepare(&sql)?;
    let allowed_paths: HashSet<String> = statement
        .query_map(params_from_iter(tags.iter()), |row| row.get::<_, String>(0))?
        .collect::<Result<_, _>>()?;

    packets.retain(|packet| {
        path_fn(packet)
            .and_then(|path| normalize_relative(path, false).ok())
            .is_some_and(|path| allowed_paths.contains(&path))
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::corpora::{CorpusConfig, CorpusEntry};

    #[test]
    fn required_tags_are_an_and_boundary_for_packet_paths() {
        let conn = rusqlite::Connection::open_in_memory().expect("open database");
        conn.execute_batch(
            "CREATE TABLE files (id INTEGER PRIMARY KEY, path TEXT NOT NULL);
             CREATE TABLE tags (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
             CREATE TABLE file_tags (file_id INTEGER NOT NULL, tag_id INTEGER NOT NULL);
             INSERT INTO files (id, path) VALUES (1, 'both.md'), (2, 'one.md');
             INSERT INTO tags (id, name) VALUES (1, 'alpha'), (2, 'beta');
             INSERT INTO file_tags (file_id, tag_id) VALUES (1, 1), (1, 2), (2, 1);",
        )
        .expect("seed tags");
        let scope = RetrievalScope {
            required_tags: vec![" alpha ".into(), "beta".into()],
            ..RetrievalScope::default()
        };
        let mut paths = vec!["both.md".to_string(), "one.md".to_string(), String::new()];

        filter_packets_by_required_tags(&conn, &mut paths, &scope, |path| {
            (!path.is_empty()).then_some(path.as_str())
        })
        .expect("filter tag scope");

        assert_eq!(paths, vec!["both.md"]);
    }
    #[test]
    fn matches_prefix_and_exact() {
        let scope = RetrievalScope {
            path_prefixes: vec!["党纪法规/".into()],
            paths: vec!["范文/样例.md".into()],
            required_tags: Vec::new(),
        };
        assert!(scope.matches_path("党纪法规/条例.md"));
        assert!(scope.matches_path("范文/样例.md"));
        assert!(!scope.matches_path("其他/笔记.md"));
        assert!(!scope.matches_path("党纪法规-归档/条例.md"));
    }

    #[test]
    fn candidate_paths_are_canonicalized_and_unsafe_candidates_fail_closed() {
        let exact = RetrievalScope {
            paths: vec!["notes/safe.md".into()],
            ..RetrievalScope::default()
        };
        assert!(exact.matches_path("notes/./safe.md"));

        let prefix = RetrievalScope {
            path_prefixes: vec!["notes/".into()],
            ..RetrievalScope::default()
        };
        assert!(!prefix.matches_path("notes/../private/secret.md"));
        assert!(!prefix.matches_path("notes/../.classified/secret.md"));
        assert!(!prefix.matches_path(".iris/runtime.md"));
    }

    #[test]
    fn path_scoped_filter_drops_packets_without_a_source_path() {
        let scope = RetrievalScope {
            path_prefixes: vec!["notes/".into()],
            paths: Vec::new(),
            required_tags: Vec::new(),
        };
        let mut packets = vec![Some("notes/in.md"), None, Some("other/out.md")];

        filter_packets_by_scope(&mut packets, &scope, |path| *path);

        assert_eq!(packets, vec![Some("notes/in.md")]);
    }

    #[test]
    fn user_scope_is_canonicalized_and_deduplicated_before_resolution() {
        let user = ContextScopeDto {
            paths: vec![" ./notes\\same.md ".into(), "notes/same.md".into()],
            path_prefixes: vec![" ./projects\\alpha ".into(), "projects/alpha/".into()],
            corpus_ids: Vec::new(),
            required_tags: vec![" #Project ".into(), "project".into()],
        };

        let scope = resolve_retrieval_scope(
            &corpus_config(),
            crate::ai_types::AgentIntent::AskNotes,
            &user,
        )
        .expect("resolve scope");

        assert_eq!(scope.paths, vec!["notes/same.md"]);
        assert_eq!(scope.path_prefixes, vec!["projects/alpha/"]);
        assert_eq!(scope.required_tags, vec!["project"]);
    }

    fn corpus_config() -> CorpusConfig {
        CorpusConfig {
            corpus: vec![
                CorpusEntry {
                    id: "authority".into(),
                    name: "制度".into(),
                    path_prefix: "制度/".into(),
                    kind: "authority".into(),
                    intents: vec!["ask_notes".into()],
                },
                CorpusEntry {
                    id: "drafts".into(),
                    name: "草稿".into(),
                    path_prefix: "草稿/".into(),
                    kind: "lookup".into(),
                    intents: Vec::new(),
                },
            ],
        }
    }

    #[test]
    fn user_scope_is_hard_boundary_over_intent_defaults() {
        let user = ContextScopeDto {
            paths: vec!["草稿/指定.md".into()],
            path_prefixes: vec!["项目/".into()],
            corpus_ids: Vec::new(),
            required_tags: Vec::new(),
        };

        let scope = resolve_retrieval_scope(
            &corpus_config(),
            crate::ai_types::AgentIntent::AskNotes,
            &user,
        )
        .expect("resolve scope");

        assert!(scope.matches_path("草稿/指定.md"));
        assert!(scope.matches_path("项目/计划.md"));
        assert!(!scope.matches_path("制度/条例.md"));
        assert_eq!(scope.path_prefixes, vec!["项目/"]);
        assert_eq!(scope.paths, vec!["草稿/指定.md"]);
    }

    #[test]
    fn explicit_corpus_ids_are_hard_boundary_over_intent_defaults() {
        let user = ContextScopeDto {
            paths: Vec::new(),
            path_prefixes: Vec::new(),
            corpus_ids: vec!["drafts".into()],
            required_tags: Vec::new(),
        };

        let scope = resolve_retrieval_scope(
            &corpus_config(),
            crate::ai_types::AgentIntent::AskNotes,
            &user,
        )
        .expect("resolve scope");

        assert!(scope.matches_path("草稿/备忘.md"));
        assert!(!scope.matches_path("制度/条例.md"));
        assert_eq!(scope.path_prefixes, vec!["草稿/"]);
    }

    #[test]
    fn unknown_corpus_ids_fail_closed() {
        let user = ContextScopeDto {
            corpus_ids: vec!["missing".into()],
            ..Default::default()
        };

        let error = resolve_retrieval_scope(
            &corpus_config(),
            crate::ai_types::AgentIntent::AskNotes,
            &user,
        )
        .expect_err("unknown corpus IDs must never become an unrestricted scope");

        assert_eq!(error.to_string(), "agent_run_invalid_retrieval_scope");
    }
}
