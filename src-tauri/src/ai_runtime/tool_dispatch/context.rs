use crate::ai_runtime::{retrieval_scope::RetrievalScope, ContextPacket, RuntimeDocumentSnapshot};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

pub struct ToolDispatchContext<'a> {
    pub note_path: Option<&'a str>,
    pub file_id: Option<i64>,
    pub web_search_enabled: bool,
    pub max_web_fetches: usize,
    pub cold_start_packets: &'a [ContextPacket],
    pub retrieval_scope: &'a RetrievalScope,
    pub runtime_documents: &'a [RuntimeDocumentSnapshot],
    pub app_handle: Option<tauri::AppHandle>,
    pub attachment_count: usize,
    pub skill_activation_plan: Option<&'a crate::ai_types::SkillActivationPlanSummary>,
}

impl<'a> ToolDispatchContext<'a> {
    pub(crate) fn ensure_active_skill_scope_allows_path(
        &self,
        db: &Database,
        path: &str,
    ) -> AppResult<()> {
        if active_skill_scope_allows_path(db, self.skill_activation_plan, path) {
            Ok(())
        } else {
            Err(AppError::msg(
                "target path is outside the confirmed Skill scope",
            ))
        }
    }
}

pub(crate) fn active_skill_scope_allows_path(
    db: &Database,
    plan: Option<&crate::ai_types::SkillActivationPlanSummary>,
    path: &str,
) -> bool {
    let Some(plan) = plan else {
        return true;
    };
    if plan.activated_skills.is_empty() {
        return true;
    }
    let normalized_path = normalize_note_path(path);
    let mut saw_scope_rule = false;
    for skill in &plan.activated_skills {
        for rule in &skill.scope_rules {
            saw_scope_rule = true;
            if skill_scope_rule_allows_path(db, &rule.kind, &rule.pattern, &normalized_path) {
                return true;
            }
        }
    }
    !saw_scope_rule
}

fn skill_scope_rule_allows_path(db: &Database, kind: &str, pattern: &str, path: &str) -> bool {
    match kind.trim().to_lowercase().as_str() {
        "path" => {
            let pattern = normalize_note_path(pattern);
            path == pattern
                || pattern
                    .strip_suffix('/')
                    .map(|prefix| path.starts_with(&format!("{prefix}/")))
                    .unwrap_or(false)
        }
        "glob" => glob_matches(&normalize_note_path(pattern), path),
        "tag" => tag_scope_allows_path(db, pattern, path),
        _ => false,
    }
}

fn tag_scope_allows_path(db: &Database, pattern: &str, path: &str) -> bool {
    let tag = normalize_tag(pattern);
    if tag.is_empty() {
        return false;
    }
    db.with_read_conn(|conn| {
        let exists = conn.query_row(
            "SELECT 1
             FROM files f
             JOIN file_tags ft ON ft.file_id = f.id
             JOIN tags t ON t.id = ft.tag_id
             WHERE lower(f.path) = lower(?1)
               AND lower(t.name) = lower(?2)
             LIMIT 1",
            rusqlite::params![path, tag],
            |_| Ok(()),
        );
        Ok(exists.is_ok())
    })
    .unwrap_or(false)
}

fn normalize_tag(value: &str) -> String {
    value.trim().trim_start_matches('#').trim().to_string()
}

fn normalize_note_path(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("./")
        .replace('\\', "/")
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/")
}

fn glob_matches(pattern: &str, value: &str) -> bool {
    fn inner(pattern: &[u8], value: &[u8]) -> bool {
        if pattern.is_empty() {
            return value.is_empty();
        }
        if pattern[0] == b'*' {
            let mut next = 1;
            while next < pattern.len() && pattern[next] == b'*' {
                next += 1;
            }
            let rest = &pattern[next..];
            return (0..=value.len()).any(|index| inner(rest, &value[index..]));
        }
        if !value.is_empty() && pattern[0] == value[0] {
            return inner(&pattern[1..], &value[1..]);
        }
        false
    }
    inner(pattern.as_bytes(), value.as_bytes())
}
