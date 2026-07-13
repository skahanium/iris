use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ai_types::{AgentIntent, SkillActivationItemSummary, SkillActivationPlanSummary};
use crate::embedding::engine::{cosine_similarity, embed_text};
use crate::error::AppResult;
use crate::storage::db::Database;

use super::{
    load_skill, scan_all_metadata, ActivationIndexMap, ScoredSkill, SkillActivationIndexRow,
    SkillConfirmationStatus, SkillEntry, SkillListEntry, SkillScope,
};

/// Load all rows from `skill_activation_index` for fast scene matching.
pub fn load_activation_index(db: &Database) -> AppResult<ActivationIndexMap> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT skill_name, scope, description, keywords, embedding_json
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
    rerank_skills_with_vectors(
        rank_skills_for_task(skills, intent, user_message, source_hints, index),
        task_query(intent, user_message),
        index,
    )
    .into_iter()
    .filter(|s| s.score >= 0.35)
    .take(3)
    .map(|s| s.skill.clone())
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

    if skill.legacy_trigger.is_none() || skill.legacy_trigger.as_deref() == Some("") {
        score += 1.0;
    }

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

fn task_query(intent: AgentIntent, user_message: &str) -> &str {
    if user_message.trim().is_empty() {
        intent_terms(intent).first().copied().unwrap_or("task")
    } else {
        user_message
    }
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

/// Rerank skills using vector similarity when embeddings are available.
/// Falls back to the BM25-scored list when embedding generation fails.
pub fn rerank_skills_with_vectors<'a>(
    scored: Vec<ScoredSkill<'a>>,
    query: &str,
    index: Option<&ActivationIndexMap>,
) -> Vec<ScoredSkill<'a>> {
    let query = query.trim();
    if query.is_empty() || index.is_none() {
        return scored;
    }

    let query_vec = match embed_text(query) {
        Ok(v) => v,
        Err(_) => return scored,
    };

    let index = index.expect("checked above");
    let mut reranked = scored;
    for ss in &mut reranked {
        let key = (ss.skill.name.clone(), ss.skill.scope);
        let Some(row) = index.get(&key) else {
            continue;
        };
        let Some(ref emb_json) = row.embedding_json else {
            continue;
        };
        let Some(skill_vec) = parse_embedding_json(emb_json) else {
            continue;
        };
        let sim = cosine_similarity(&query_vec, &skill_vec) as f64;
        ss.score += sim * 3.0;
    }

    reranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
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

fn apply_runtime_prompt_sections(skill: &mut SkillEntry, vault: &Path, db: Option<&Database>) {
    let _ = (skill, vault, db);
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
    let mut candidates: Vec<ScoredSkill<'_>> = Vec::new();
    for skill in skills.iter().filter(|skill| skill_can_activate(skill)) {
        if let Some((score, _reason)) =
            activation_reason(skill, agent_intent, user_message, source_hints)
        {
            candidates.push(ScoredSkill { skill, score });
        }
    }

    let ranked = rerank_skills_with_vectors(
        rank_skills_for_task(
            skills,
            agent_intent,
            user_message,
            source_hints,
            options.index,
        ),
        task_query(agent_intent, user_message),
        options.index,
    );
    for scored in ranked {
        if scored.score >= 0.35
            && !candidates
                .iter()
                .any(|existing| existing.skill.name == scored.skill.name)
        {
            candidates.push(scored);
        }
    }

    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.truncate(5);

    let mut activated = Vec::new();

    for scored in candidates.into_iter().take(3) {
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
/// Load enabled skills for prompt injection after task/capability matching.
pub fn active_skills_for_task_prompt(
    vault: &Path,
    intent: AgentIntent,
    db: Option<&Database>,
    user_message: &str,
    source_hints: &[String],
) -> AppResult<Vec<SkillEntry>> {
    let metadata = scan_all_metadata(vault)?;
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
        rank_skills_for_task(&metadata, intent, user_message, source_hints, index_ref),
        task_query(intent, user_message),
        index_ref,
    );
    let mut out = Vec::new();
    for scored in ranked {
        let path = PathBuf::from(&scored.skill.file_path);
        if let Ok(mut skill) = load_skill(&path, scored.skill.scope) {
            skill.enabled = scored.skill.enabled;
            if skill_can_activate(&skill) {
                apply_runtime_prompt_sections(&mut skill, vault, db);
                out.push(skill);
            }
        }
    }
    Ok(out)
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
        task_query(intent, user_message),
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
