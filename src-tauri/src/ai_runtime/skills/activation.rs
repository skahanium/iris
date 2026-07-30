use std::collections::{HashMap, HashSet};

use rusqlite::params;
use sha2::{Digest, Sha256};

use crate::ai_types::{AgentIntent, SkillActivationItemSummary, SkillActivationPlanSummary};
use crate::embedding::engine::{
    cosine_similarity, EMBEDDING_DIMENSION, EMBEDDING_MODEL_FINGERPRINT,
};
use crate::error::AppResult;
use crate::storage::db::Database;

use super::{
    ActivationIndexMap, ScoredSkill, SkillActivationIndexRow, SkillConfirmationStatus, SkillEntry,
    SkillListEntry, SkillScope,
};

/// A run may activate one primary skill and at most one auxiliary skill.
const MAX_ACTIVATED_SKILLS: usize = 2;
/// Enabled only while the checked-in activation evaluation improves recall
/// without adding high-risk false activations over the lexical baseline.
pub(crate) const SKILL_VECTOR_RERANK_DEFAULT_ENABLED: bool = true;

/// Load all rows from `skill_activation_index` for fast scene matching.
pub fn load_activation_index(db: &Database) -> AppResult<ActivationIndexMap> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT skill_name, scope, description, keywords, embedding_json,
                    embedding_source_hash, embedding_model, embedding_dimensions
             FROM skill_activation_index",
        )?;
        let rows = stmt.query_map([], |row| {
            let scope_str: String = row.get(1)?;
            Ok(SkillActivationIndexRow {
                skill_name: row.get(0)?,
                scope: if scope_str == "Vault" {
                    SkillScope::Vault
                } else {
                    SkillScope::Global
                },
                description: row.get(2)?,
                keywords: row.get(3)?,
                embedding_json: row.get(4)?,
                embedding_source_hash: row.get(5)?,
                embedding_model: row.get(6)?,
                embedding_dimensions: row.get(7)?,
            })
        })?;
        let mut map = ActivationIndexMap::new();
        for row in rows {
            let row = row?;
            map.insert((row.skill_name.clone(), row.scope), row);
        }
        Ok(map)
    })
}

/// Rebuild the activation metadata for the one vault whose Skills are loaded
/// into the runtime cache. This is called only during vault activation and
/// explicit user refresh/confirmation, never from a Run.
///
/// The index intentionally persists descriptions and declared trigger keywords
/// only. Full Skill instruction bodies stay in the in-memory, confirmed cache
/// and are selected for prompt injection only after the Run plan is built.
pub fn rebuild_activation_index(db: &Database, skills: &[SkillEntry]) -> AppResult<()> {
    db.with_conn(|conn| {
        let transaction = conn.unchecked_transaction()?;
        let mut statement = transaction.prepare(
            "INSERT INTO skill_activation_index
             (skill_name, scope, description, keywords, embedding_json,
              embedding_source_hash, embedding_model, embedding_dimensions, updated_at)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, NULL, NULL, datetime('now'))
             ON CONFLICT(skill_name, scope) DO UPDATE SET
                 description = excluded.description,
                 keywords = excluded.keywords,
                 embedding_json = CASE
                     WHEN skill_activation_index.embedding_source_hash =
                          excluded.embedding_source_hash
                     THEN skill_activation_index.embedding_json
                     ELSE NULL
                 END,
                 embedding_model = CASE
                     WHEN skill_activation_index.embedding_source_hash =
                          excluded.embedding_source_hash
                     THEN skill_activation_index.embedding_model
                     ELSE NULL
                 END,
                 embedding_dimensions = CASE
                     WHEN skill_activation_index.embedding_source_hash =
                          excluded.embedding_source_hash
                     THEN skill_activation_index.embedding_dimensions
                     ELSE NULL
                 END,
                 embedding_source_hash = excluded.embedding_source_hash,
                 updated_at = CASE
                     WHEN skill_activation_index.embedding_source_hash =
                          excluded.embedding_source_hash
                      AND skill_activation_index.description IS excluded.description
                      AND skill_activation_index.keywords IS excluded.keywords
                     THEN skill_activation_index.updated_at
                     ELSE datetime('now')
                 END",
        )?;
        let mut desired = HashSet::with_capacity(skills.len());
        for skill in skills {
            let scope = scope_wire(skill.scope);
            let keywords = activation_index_keywords(skill);
            let source_hash =
                activation_embedding_source_hash(&skill.name, &skill.description, &keywords);
            statement.execute(params![
                skill.name,
                scope,
                skill.description,
                keywords,
                source_hash,
            ])?;
            desired.insert((skill.name.clone(), scope_wire(skill.scope)));
        }
        drop(statement);

        let existing = {
            let mut statement =
                transaction.prepare("SELECT skill_name, scope FROM skill_activation_index")?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for (skill_name, scope) in existing {
            if !desired.contains(&(skill_name.clone(), scope.clone())) {
                transaction.execute(
                    "DELETE FROM skill_activation_index
                     WHERE skill_name = ?1 AND scope = ?2",
                    params![skill_name, scope],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    })
}

fn activation_embedding_source_hash(name: &str, description: &str, keywords: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(activation_embedding_source(name, description, keywords));
    hex::encode(hasher.finalize())
}

pub(crate) fn activation_embedding_source(name: &str, description: &str, keywords: &str) -> String {
    format!(
        "name: {}\ndescription: {}\nkeywords: {}",
        name.trim(),
        description.trim(),
        keywords.trim()
    )
}

fn activation_index_keywords(skill: &SkillEntry) -> String {
    let mut keywords = skill.trigger_hints();
    keywords.extend(skill_declared_keywords(skill));
    keywords.sort();
    keywords.dedup();
    keywords.join(" ")
}

fn skill_declared_keywords(skill: &SkillEntry) -> Vec<String> {
    let Some(value) = skill.metadata.get("keywords") else {
        return Vec::new();
    };
    match value {
        serde_json::Value::String(raw) => raw
            .split(|character: char| character.is_whitespace() || character == ',')
            .filter(|keyword| !keyword.is_empty())
            .map(str::to_owned)
            .collect(),
        serde_json::Value::Array(values) => values
            .iter()
            .filter_map(serde_json::Value::as_str)
            .filter(|keyword| !keyword.is_empty())
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_embedding_json(raw: &str) -> Option<Vec<f32>> {
    serde_json::from_str::<Vec<f32>>(raw).ok()
}

/// Filter and rank enabled skills by task intent and capability affinity.
pub fn skills_for_task(
    skills: &[SkillEntry],
    intent: AgentIntent,
    user_message: &str,
    source_hints: &[String],
    index: Option<&ActivationIndexMap>,
) -> Vec<SkillEntry> {
    select_skills_for_activation(skills, intent, user_message, source_hints, index, None)
        .into_iter()
        .map(|scored| scored.skill.clone())
        .collect()
}

/// Scored ranking with optional activation-index overlay.
pub fn rank_skills_for_task<'a>(
    skills: &'a [SkillEntry],
    intent: AgentIntent,
    user_message: &str,
    source_hints: &[String],
    index: Option<&ActivationIndexMap>,
) -> Vec<ScoredSkill<'a>> {
    let task_terms = task_terms(intent, user_message, source_hints);

    let mut scored: Vec<ScoredSkill<'a>> = skills
        .iter()
        .filter(|s| skill_can_activate(s))
        .filter_map(|s| {
            let index_row = index.and_then(|m| m.get(&(s.name.clone(), s.scope)));
            let score = compute_skill_score(s, &task_terms, index_row);
            if score > 0.0 {
                Some(ScoredSkill { skill: s, score })
            } else {
                None
            }
        })
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.skill.name.cmp(&b.skill.name))
            .then_with(|| scope_wire(a.skill.scope).cmp(&scope_wire(b.skill.scope)))
    });
    scored
}

fn skill_can_activate(skill: &SkillEntry) -> bool {
    skill.enabled && skill.confirmation_status == SkillConfirmationStatus::Confirmed
}

/// BM25-style scoring for a single skill against task terms.
fn compute_skill_score(
    skill: &SkillEntry,
    task_terms: &[String],
    index_row: Option<&SkillActivationIndexRow>,
) -> f64 {
    let mut score: f64 = 0.0;

    if let Some(trigger) = &skill.legacy_trigger {
        let t = trigger.to_lowercase();
        for term in task_terms {
            if t.contains(term) {
                score += 3.0;
                break;
            }
        }
    }

    let description = index_row
        .and_then(|r| r.description.as_deref())
        .filter(|d| !d.is_empty())
        .unwrap_or(skill.description.as_str());
    let index_keywords = index_row
        .and_then(|r| r.keywords.as_deref())
        .unwrap_or("")
        .to_lowercase();

    let desc_lower = description.to_lowercase();
    let name_lower = skill.name.to_lowercase();
    let content_lower = skill.content.to_lowercase();

    for term in task_terms {
        let desc_tf = desc_lower.matches(term).count() as f64;
        if desc_tf > 0.0 {
            score += (desc_tf / (desc_tf + 1.2)) * 3.0;
        }
        if name_lower.contains(term) {
            score += 4.0;
        }
        let content_tf = content_lower.matches(term).count() as f64;
        if content_tf > 0.0 {
            score += (content_tf / (content_tf + 1.2)) * 0.5;
        }
        if index_keywords.contains(term) {
            score += 2.5;
        }
    }

    if let Some(keywords) = skill.metadata.get("keywords") {
        if let Some(kw_str) = keywords.as_str() {
            let kw_lower = kw_str.to_lowercase();
            for term in task_terms {
                if kw_lower.contains(term) {
                    score += 2.0;
                }
            }
        }
    }

    score
}

fn task_terms(intent: AgentIntent, user_message: &str, source_hints: &[String]) -> Vec<String> {
    let mut terms: Vec<String> = intent_terms(intent).iter().map(|s| s.to_string()).collect();
    for token in user_message
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .map(str::to_lowercase)
        .filter(|token| token.len() >= 3)
    {
        push_term(&mut terms, token);
    }
    for hint in source_hints {
        for token in hint
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .map(str::to_lowercase)
            .filter(|token| token.len() >= 3)
        {
            push_term(&mut terms, token);
        }
    }
    terms
}

fn push_term(terms: &mut Vec<String>, term: String) {
    if !terms.contains(&term) {
        terms.push(term);
    }
}

/// Rerank existing lexical candidates using already-prepared vector data.
///
/// This function never loads a model. Missing queries, malformed vectors, stale
/// source hashes, model changes, and dimension mismatches all preserve the
/// deterministic lexical order.
pub fn rerank_skills_with_vectors<'a>(
    scored: Vec<ScoredSkill<'a>>,
    query_embedding: Option<&[f32]>,
    index: Option<&ActivationIndexMap>,
) -> Vec<ScoredSkill<'a>> {
    let Some(query_embedding) = query_embedding else {
        return scored;
    };
    if query_embedding.len() != EMBEDDING_DIMENSION || index.is_none() {
        return scored;
    }

    let index = index.expect("checked above");
    let mut similarities = Vec::with_capacity(scored.len());
    for scored_skill in &scored {
        let key = (scored_skill.skill.name.clone(), scored_skill.skill.scope);
        let Some(row) = index.get(&key) else {
            return scored;
        };
        let Some(ref embedding_json) = row.embedding_json else {
            return scored;
        };
        let expected_source_hash = activation_embedding_source_hash(
            &scored_skill.skill.name,
            &scored_skill.skill.description,
            &activation_index_keywords(scored_skill.skill),
        );
        if row.embedding_source_hash != expected_source_hash
            || row.embedding_model.as_deref() != Some(EMBEDDING_MODEL_FINGERPRINT)
            || row.embedding_dimensions != Some(EMBEDDING_DIMENSION as i64)
        {
            return scored;
        }
        let Some(skill_vector) = parse_embedding_json(embedding_json) else {
            return scored;
        };
        if skill_vector.len() != EMBEDDING_DIMENSION {
            return scored;
        }
        similarities.push(cosine_similarity(query_embedding, &skill_vector) as f64);
    }

    let mut reranked = scored;
    for (scored_skill, similarity) in reranked.iter_mut().zip(similarities) {
        scored_skill.score += similarity * 3.0;
    }

    reranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.skill.name.cmp(&b.skill.name))
            .then_with(|| scope_wire(a.skill.scope).cmp(&scope_wire(b.skill.scope)))
    });
    reranked
}

fn intent_terms(intent: AgentIntent) -> &'static [&'static str] {
    match intent {
        AgentIntent::Chat => &["chat", "assistant"],
        AgentIntent::AskNotes => &["ask_notes", "knowledge", "lookup", "notes"],
        AgentIntent::RewriteSelection | AgentIntent::Write => &["write", "rewrite", "draft"],
        AgentIntent::Research => &["research", "evidence", "synthesis"],
        AgentIntent::Organize => &["organize", "tags", "folders", "links"],
        AgentIntent::CitationCheck => &["citation", "fact", "claim"],
        AgentIntent::Chapter => &["chapter", "outline", "structure"],
        AgentIntent::DocumentCheck => &["document", "style", "outline"],
        AgentIntent::VisionChat => &["vision", "image"],
        AgentIntent::SkillManagement => &["skill", "create", "confirm"],
    }
}

fn scope_wire(scope: SkillScope) -> String {
    match scope {
        SkillScope::Global => "Global".into(),
        SkillScope::Vault => "Vault".into(),
    }
}

pub fn filter_skill_content_to_injected_sections(
    skill: &mut SkillEntry,
    injected_sections: &[String],
) -> AppResult<()> {
    let _ = (skill, injected_sections);
    Ok(())
}

fn activation_reason(
    skill: &SkillEntry,
    intent: AgentIntent,
    user_message: &str,
    source_hints: &[String],
) -> Option<(f64, String)> {
    let msg = user_message.to_lowercase();
    let name = skill.name.to_lowercase();
    if msg.contains(&name) {
        return Some((100.0, "explicit_skill_mention".into()));
    }
    if matches!(intent, AgentIntent::SkillManagement)
        && (name.contains("skill") || skill.trigger_hints().iter().any(|h| h.contains("skill")))
    {
        return Some((80.0, "skill_management_intent".into()));
    }
    let trigger_hints = skill.trigger_hints();
    if trigger_hints
        .iter()
        .any(|hint| !hint.is_empty() && msg.contains(&hint.to_lowercase()))
    {
        return Some((70.0, "trigger_hint".into()));
    }
    if trigger_hints.iter().any(|hint| {
        source_hints
            .iter()
            .any(|source| source.to_lowercase().contains(&hint.to_lowercase()))
    }) {
        return Some((65.0, "source_hint".into()));
    }
    let declared_keywords = skill_declared_keywords(skill);
    if declared_keywords
        .iter()
        .any(|keyword| !keyword.is_empty() && msg.contains(&keyword.to_lowercase()))
    {
        return Some((60.0, "keyword_match".into()));
    }
    if declared_keywords.iter().any(|keyword| {
        source_hints
            .iter()
            .any(|source| source.to_lowercase().contains(&keyword.to_lowercase()))
    }) {
        return Some((58.0, "keyword_source_hint".into()));
    }
    let terms = intent_terms(intent);
    let haystack = format!(
        "{} {} {}",
        skill.name.to_lowercase(),
        skill.description.to_lowercase(),
        skill.trigger_hints().join(" ").to_lowercase()
    );
    if terms.iter().any(|term| haystack.contains(term)) {
        return Some((55.0, "intent_term_match".into()));
    }
    None
}

#[derive(Clone, Copy)]
struct SkillActivationBuildOptions<'a> {
    index: Option<&'a ActivationIndexMap>,
    query_embedding: Option<&'a [f32]>,
    db: Option<&'a Database>,
    enable_manifest_gating: bool,
}
/// Build a safe, per-run skill activation plan from task facts.
pub fn build_skill_activation_plan_for_task(
    skills: &[SkillEntry],
    agent_intent: AgentIntent,
    user_message: &str,
    source_hints: &[String],
    index: Option<&ActivationIndexMap>,
) -> SkillActivationPlanSummary {
    build_skill_activation_plan_for_task_inner(
        skills,
        agent_intent,
        user_message,
        source_hints,
        SkillActivationBuildOptions {
            index,
            query_embedding: None,
            db: None,
            enable_manifest_gating: false,
        },
    )
}

/// Build a safe activation plan with a query vector prepared before Run execution.
pub fn build_skill_activation_plan_for_task_with_query_embedding(
    skills: &[SkillEntry],
    agent_intent: AgentIntent,
    user_message: &str,
    source_hints: &[String],
    index: Option<&ActivationIndexMap>,
    query_embedding: Option<&[f32]>,
) -> SkillActivationPlanSummary {
    build_skill_activation_plan_for_task_inner(
        skills,
        agent_intent,
        user_message,
        source_hints,
        SkillActivationBuildOptions {
            index,
            query_embedding,
            db: None,
            enable_manifest_gating: false,
        },
    )
}

/// Build a skill activation plan that evaluates typed manifest sections against runtime state.
pub fn build_skill_activation_plan_for_task_with_runtime(
    skills: &[SkillEntry],
    agent_intent: AgentIntent,
    user_message: &str,
    source_hints: &[String],
    index: Option<&ActivationIndexMap>,
    db: Option<&Database>,
) -> SkillActivationPlanSummary {
    build_skill_activation_plan_for_task_inner(
        skills,
        agent_intent,
        user_message,
        source_hints,
        SkillActivationBuildOptions {
            index,
            query_embedding: None,
            db,
            enable_manifest_gating: true,
        },
    )
}

fn build_skill_activation_plan_for_task_inner(
    skills: &[SkillEntry],
    agent_intent: AgentIntent,
    user_message: &str,
    source_hints: &[String],
    options: SkillActivationBuildOptions<'_>,
) -> SkillActivationPlanSummary {
    let mut activated = Vec::new();

    for scored in select_skills_for_activation(
        skills,
        agent_intent,
        user_message,
        source_hints,
        options.index,
        options.query_embedding,
    ) {
        let skill = scored.skill;
        let reason = activation_reason(skill, agent_intent, user_message, source_hints)
            .map(|(_, reason)| reason)
            .unwrap_or_else(|| "task_prompt_or_vector_match".into());
        let _ = (options.enable_manifest_gating, options.db);
        activated.push(SkillActivationItemSummary {
            name: skill.name.clone(),
            scope: scope_wire(skill.scope),
            scope_rules: skill.scope_rules.clone(),
            score: scored.score,
            match_reason: reason,
            injected_sections: vec!["skill_overlay".into()],
            degraded_reasons: Vec::new(),
            requested_tools: Vec::new(),
            confirmation_required_tools: Vec::new(),
            blocked_capabilities: Vec::new(),
        });
    }

    SkillActivationPlanSummary {
        skill_overlay_summary: if activated.is_empty() {
            "No skills activated for this run.".into()
        } else {
            format!("{} prompt-only skill(s) activated.", activated.len())
        },
        activated_skills: activated,
        requested_tools: Vec::new(),
        confirmation_required_tools: Vec::new(),
        degraded: false,
        blocked_capabilities: Vec::new(),
    }
}

/// Resolve the exact, already-loaded instruction bodies for a run plan.
///
/// This function deliberately performs no filesystem or database I/O. Callers must build the
/// activation plan from their cached skill registry and activation index, then pass the same
/// loaded entries here before prompt construction.
pub fn activated_skills_from_plan(
    plan: &SkillActivationPlanSummary,
    available_skills: &[SkillEntry],
) -> Vec<SkillEntry> {
    plan.activated_skills
        .iter()
        .take(MAX_ACTIVATED_SKILLS)
        .filter_map(|planned| {
            available_skills
                .iter()
                .find(|skill| {
                    skill.name == planned.name
                        && scope_wire(skill.scope) == planned.scope
                        && skill_can_activate(skill)
                })
                .cloned()
        })
        .collect()
}

fn select_skills_for_activation<'a>(
    skills: &'a [SkillEntry],
    intent: AgentIntent,
    user_message: &str,
    source_hints: &[String],
    index: Option<&ActivationIndexMap>,
    query_embedding: Option<&[f32]>,
) -> Vec<ScoredSkill<'a>> {
    let mut strong_candidates: Vec<ScoredSkill<'_>> = skills
        .iter()
        .filter(|skill| skill_can_activate(skill))
        .filter_map(|skill| {
            activation_reason(skill, intent, user_message, source_hints)
                .filter(|(score, _reason)| *score > 55.0)
                .map(|(score, _reason)| ScoredSkill { skill, score })
        })
        .collect();
    strong_candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.skill.name.cmp(&b.skill.name))
            .then_with(|| scope_wire(a.skill.scope).cmp(&scope_wire(b.skill.scope)))
    });

    let ranked = rerank_skills_with_vectors(
        rank_skills_for_task(skills, intent, user_message, source_hints, index),
        query_embedding,
        index,
    );
    for scored in ranked {
        if scored.score >= 0.35
            && !strong_candidates.iter().any(|existing| {
                existing.skill.name == scored.skill.name
                    && existing.skill.scope == scored.skill.scope
            })
        {
            strong_candidates.push(scored);
        }
    }

    strong_candidates.truncate(MAX_ACTIVATED_SKILLS);
    strong_candidates
}

/// Annotate list entries with task affinity when an intent is provided.
pub fn enrich_list_with_task(
    mut entries: Vec<SkillListEntry>,
    intent: AgentIntent,
    user_message: &str,
    source_hints: &[String],
    db: Option<&Database>,
) -> AppResult<Vec<SkillListEntry>> {
    let skills: Vec<SkillEntry> = entries.iter().map(|e| e.skill.clone()).collect();
    let index_map = db
        .map(load_activation_index)
        .transpose()?
        .unwrap_or_default();
    let index_ref = if index_map.is_empty() {
        None
    } else {
        Some(&index_map)
    };
    let ranked = rerank_skills_with_vectors(
        rank_skills_for_task(&skills, intent, user_message, source_hints, index_ref),
        None,
        index_ref,
    );
    let score_map: HashMap<(String, SkillScope), f64> = ranked
        .iter()
        .map(|s| ((s.skill.name.clone(), s.skill.scope), s.score))
        .collect();

    for entry in &mut entries {
        let key = (entry.skill.name.clone(), entry.skill.scope);
        entry.task_active = Some(score_map.contains_key(&key));
        entry.task_score = score_map.get(&key).copied();
    }
    Ok(entries)
}
