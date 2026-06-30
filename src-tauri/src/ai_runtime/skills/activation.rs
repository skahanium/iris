use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use crate::ai_runtime::{
    agent_task_policy::intent_from_legacy_scene, capability_resolver::resolve_required_capability,
    tool_catalog::catalog_find, AiScene,
};
use crate::ai_types::{
    AgentIntent, SkillActivationItemSummary, SkillActivationPlanSummary,
    SkillResourceStatusSummary, SkillRuntimeCapability, ToolCapabilityAffinity,
};
use crate::embedding::engine::{cosine_similarity, embed_text};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

use super::compatibility_impl::blocked_capabilities_for_skill;
use super::manifest_impl::load_manifest_for_skill_dir;
use super::resources_impl::{
    effective_optional_resources_for_skill, effective_required_resources_for_skill,
    ALLOWED_RESOURCE_DIRS, MAX_SKILL_RESOURCE_CHARS,
};
use super::validation_impl::confirmation_required_tools;
use super::workspace_impl::{workspace_root_relative, workspace_status_for_skill};
use super::{
    load_skill, scan_all_metadata, ActivationIndexMap, ScoredSkill, SkillActivationIndexRow,
    SkillEntry, SkillListEntry, SkillScope,
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
///
/// When `skill_activation_index` rows are supplied, keywords/description from the
/// index take precedence over file metadata for matching.
pub fn skills_for_scene(
    skills: &[SkillEntry],
    scene: AiScene,
    user_message: &str,
) -> Vec<SkillEntry> {
    skills_for_task(
        skills,
        intent_from_legacy_scene(scene),
        user_message,
        &[],
        None,
    )
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

/// Legacy scored version of `skills_for_scene`; retained for migration-only callers.
pub fn rank_skills_for_scene<'a>(skills: &'a [SkillEntry], scene: AiScene) -> Vec<ScoredSkill<'a>> {
    rank_skills_for_task(
        skills,
        intent_from_legacy_scene(scene),
        scene.profile(),
        &[],
        None,
    )
}

/// Legacy scored ranking with optional activation-index overlay.
pub fn rank_skills_for_scene_with_index<'a>(
    skills: &'a [SkillEntry],
    scene: AiScene,
    index: Option<&ActivationIndexMap>,
) -> Vec<ScoredSkill<'a>> {
    rank_skills_for_task(
        skills,
        intent_from_legacy_scene(scene),
        scene.profile(),
        &[],
        index,
    )
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
        .filter(|s| s.enabled)
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

    for term in capability_terms_for_skill(skill) {
        if task_terms.contains(&term) {
            score += 2.5;
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

fn capability_terms_for_skill(skill: &SkillEntry) -> Vec<String> {
    let mut terms = Vec::new();
    for tool in &skill.allowed_tools {
        let Some(entry) = catalog_find(tool) else {
            continue;
        };
        for capability in entry.capability_affinity() {
            for term in capability_terms(capability) {
                push_term(&mut terms, term.to_string());
            }
        }
    }
    for capability in skill.requested_capabilities() {
        match capability {
            SkillRuntimeCapability::ReadResource => push_term(&mut terms, "read".into()),
            SkillRuntimeCapability::WriteStorage => push_term(&mut terms, "write".into()),
            SkillRuntimeCapability::RequestCapabilities => push_term(&mut terms, "skill".into()),
            SkillRuntimeCapability::ExecuteScriptSandboxed
            | SkillRuntimeCapability::InstallDependency
            | SkillRuntimeCapability::McpBridge => push_term(&mut terms, "blocked".into()),
        }
    }
    terms
}

fn capability_terms(capability: ToolCapabilityAffinity) -> &'static [&'static str] {
    match capability {
        ToolCapabilityAffinity::ReadNotes => &["read", "notes", "knowledge"],
        ToolCapabilityAffinity::SearchNotes => &["search", "notes", "knowledge"],
        ToolCapabilityAffinity::WriteNotes => &["write", "draft", "rewrite"],
        ToolCapabilityAffinity::PatchDocument => &["patch", "document", "write"],
        ToolCapabilityAffinity::WebFetch => &["web", "fetch", "research"],
        ToolCapabilityAffinity::ResearchSynthesis => &["research", "evidence", "synthesis"],
        ToolCapabilityAffinity::SkillManagement => &["skill", "install", "update"],
        ToolCapabilityAffinity::VaultOrganize => &["organize", "tags", "folders"],
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
        AgentIntent::SkillManagement => &["skill", "install", "update", "toggle"],
    }
}

fn scope_wire(scope: SkillScope) -> String {
    match scope {
        SkillScope::Global => "Global".into(),
        SkillScope::Vault => "Vault".into(),
    }
}

fn build_resource_summary(
    skill_root: Option<&Path>,
    relative_path: String,
    kind: &str,
) -> SkillResourceStatusSummary {
    let invalid = |reason: &str| SkillResourceStatusSummary {
        relative_path: relative_path.clone(),
        kind: kind.into(),
        available: false,
        size_bytes: None,
        truncated: false,
        reason: Some(reason.into()),
    };
    let rel = Path::new(relative_path.trim_start_matches('/'));
    if relative_path.trim().is_empty() || rel.is_absolute() || relative_path.contains("..") {
        return invalid("invalid resource path");
    }
    let Some(top) = rel.components().next().and_then(|c| c.as_os_str().to_str()) else {
        return invalid("invalid resource path");
    };
    if !ALLOWED_RESOURCE_DIRS.contains(&top) {
        return invalid("outside allowed resource directories");
    }
    let Some(skill_root) = skill_root else {
        return invalid("skill root unavailable");
    };
    let root_canonical = match skill_root.canonicalize() {
        Ok(path) => path,
        Err(_) => return invalid("skill root unavailable"),
    };
    let target = skill_root.join(rel);
    let canonical = match target.canonicalize() {
        Ok(path) => path,
        Err(_) => return invalid("not found"),
    };
    if !canonical.starts_with(&root_canonical) {
        return invalid("resource path escapes skill root");
    }
    let metadata = match std::fs::metadata(&canonical) {
        Ok(metadata) if metadata.is_file() => metadata,
        Ok(_) => return invalid("resource is not a file"),
        Err(_) => return invalid("not found"),
    };
    let size_bytes = metadata.len();
    SkillResourceStatusSummary {
        relative_path,
        kind: kind.into(),
        available: true,
        size_bytes: Some(size_bytes),
        truncated: size_bytes as usize > MAX_SKILL_RESOURCE_CHARS,
        reason: Some("available".into()),
    }
}

fn build_resource_summaries(skill: &SkillEntry) -> Vec<SkillResourceStatusSummary> {
    let skill_root = Path::new(&skill.file_path).parent();
    effective_required_resources_for_skill(skill)
        .into_iter()
        .map(|relative_path| build_resource_summary(skill_root, relative_path, "required"))
        .chain(
            effective_optional_resources_for_skill(skill)
                .into_iter()
                .map(|relative_path| build_resource_summary(skill_root, relative_path, "optional")),
        )
        .collect()
}

#[derive(Debug, Clone, Default)]
struct PromptSectionEvaluation {
    injected_sections: Vec<String>,
    prompt_content: String,
    degraded_reasons: Vec<String>,
}

fn skill_root_for_entry(skill: &SkillEntry) -> Option<&Path> {
    Path::new(&skill.file_path).parent()
}

fn safe_section_source_path(skill_root: &Path, source: &str) -> AppResult<PathBuf> {
    let rel = Path::new(source.trim_start_matches('/'));
    if source.trim().is_empty()
        || rel.is_absolute()
        || rel.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(AppError::msg(
            "prompt section source escapes skill directory",
        ));
    }
    Ok(skill_root.join(rel))
}

fn read_prompt_section_source(skill: &SkillEntry, source: &str) -> AppResult<String> {
    let Some(skill_root) = skill_root_for_entry(skill) else {
        return Err(AppError::msg("skill root unavailable"));
    };
    if source == "SKILL.md" {
        return Ok(skill.content.clone());
    }
    let path = safe_section_source_path(skill_root, source)?;
    let root = skill_root.canonicalize()?;
    let canonical = path.canonicalize()?;
    if !canonical.starts_with(root) {
        return Err(AppError::msg(
            "prompt section source escapes skill directory",
        ));
    }
    Ok(std::fs::read_to_string(canonical)?.trim().to_string())
}

fn runtime_ready_for_manifest(
    manifest: &super::manifest_impl::IrisSkillManifest,
    db: Option<&Database>,
) -> (bool, Vec<String>) {
    let required_profiles: Vec<&str> = manifest
        .mcp
        .dependencies
        .iter()
        .filter(|dependency| dependency.required)
        .map(|dependency| dependency.profile_id.as_str())
        .collect();
    if required_profiles.is_empty() {
        return (true, Vec::new());
    }
    let Some(db) = db else {
        return (
            false,
            vec!["MCP runtime registry is unavailable for this run".into()],
        );
    };
    let profiles = match crate::ai_runtime::mcp_runtime_registry::list_runtime_profiles(db) {
        Ok(profiles) => profiles,
        Err(err) => {
            return (
                false,
                vec![format!("MCP runtime registry unavailable: {err}")],
            )
        }
    };
    let mut reasons = Vec::new();
    for profile_id in required_profiles {
        match profiles.iter().find(|profile| profile.id == profile_id) {
            Some(profile)
                if profile.enabled
                    && profile.status
                        == crate::ai_runtime::mcp_runtime_registry::McpRuntimeStatus::Ready => {}
            Some(profile) => reasons.push(profile.last_error.clone().unwrap_or_else(|| {
                format!("MCP profile {profile_id} is {}", profile.status.as_str())
            })),
            None => reasons.push(format!("MCP profile {profile_id} is not configured")),
        }
    }
    (reasons.is_empty(), reasons)
}

fn evaluate_prompt_sections(
    skill: &SkillEntry,
    vault_root: Option<&Path>,
    db: Option<&Database>,
    selected_section_ids: Option<&[String]>,
) -> Option<PromptSectionEvaluation> {
    let skill_root = skill_root_for_entry(skill)?;
    let outcome = load_manifest_for_skill_dir(skill_root, None).ok()?;
    let manifest = &outcome.manifest;
    if manifest.prompt.sections.is_empty() {
        return None;
    }

    let selected: Vec<String> = selected_section_ids
        .map(|ids| {
            ids.iter()
                .filter(|id| id.as_str() != "skill_overlay")
                .cloned()
                .collect()
        })
        .unwrap_or_else(|| {
            if manifest.prompt.default_sections.is_empty() {
                manifest
                    .prompt
                    .sections
                    .iter()
                    .map(|section| section.id.clone())
                    .collect()
            } else {
                manifest.prompt.default_sections.clone()
            }
        });
    let (runtime_ready, runtime_reasons) = runtime_ready_for_manifest(manifest, db);
    let workspace_status = vault_root.map(|vault| workspace_status_for_skill(vault, skill));

    let mut evaluation = PromptSectionEvaluation::default();
    for section in manifest
        .prompt
        .sections
        .iter()
        .filter(|section| selected.contains(&section.id))
    {
        let mut section_reasons = Vec::new();
        if section.requires_runtime && !runtime_ready {
            section_reasons.push(format!(
                "section `{}` skipped: runtime unavailable",
                section.id
            ));
            section_reasons.extend(runtime_reasons.iter().cloned());
        }
        if section.requires_workspace
            && workspace_status
                .as_ref()
                .is_some_and(|status| !status.workspace_ready)
        {
            section_reasons.push(format!(
                "section `{}` skipped: workspace is not prepared",
                section.id
            ));
        }
        for resource in &section.requires_resources {
            let summary = build_resource_summary(Some(skill_root), resource.clone(), "required");
            if !summary.available {
                section_reasons.push(format!(
                    "section `{}` skipped: required resource `{}` is unavailable",
                    section.id, resource
                ));
            }
        }
        for capability in &section.requires_capabilities {
            match db {
                Some(db) => {
                    if let Err(err) = resolve_required_capability(db, capability) {
                        section_reasons.push(format!(
                            "section `{}` skipped: required capability `{}` is unavailable: {}",
                            section.id,
                            err.capability,
                            err.reason_code()
                        ));
                    }
                }
                None => section_reasons.push(format!(
                    "section `{}` skipped: required capability `{}` cannot be resolved without runtime registry",
                    section.id, capability
                )),
            }
        }
        if !section_reasons.is_empty() {
            for reason in section_reasons {
                if !evaluation.degraded_reasons.contains(&reason) {
                    evaluation.degraded_reasons.push(reason);
                }
            }
            continue;
        }
        match read_prompt_section_source(skill, &section.source) {
            Ok(content) => {
                if !content.trim().is_empty() {
                    if !evaluation.prompt_content.is_empty() {
                        evaluation.prompt_content.push_str("\n\n");
                    }
                    evaluation.prompt_content.push_str(content.trim());
                }
                evaluation.injected_sections.push(section.id.clone());
            }
            Err(err) => evaluation.degraded_reasons.push(format!(
                "section `{}` skipped: source unavailable: {err}",
                section.id
            )),
        }
    }
    Some(evaluation)
}

pub fn filter_skill_content_to_injected_sections(
    skill: &mut SkillEntry,
    injected_sections: &[String],
) -> AppResult<()> {
    let Some(evaluation) = evaluate_prompt_sections(skill, None, None, Some(injected_sections))
    else {
        return Ok(());
    };
    skill.content = evaluation.prompt_content;
    Ok(())
}

fn apply_runtime_prompt_sections(skill: &mut SkillEntry, vault: &Path, db: Option<&Database>) {
    if let Some(evaluation) = evaluate_prompt_sections(skill, Some(vault), db, None) {
        skill.content = evaluation.prompt_content;
    }
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

/// Build a safe, per-run skill activation plan.
pub fn build_skill_activation_plan(
    skills: &[SkillEntry],
    scene: AiScene,
    agent_intent: AgentIntent,
    user_message: &str,
    source_hints: &[String],
    index: Option<&ActivationIndexMap>,
) -> SkillActivationPlanSummary {
    build_skill_activation_plan_for_task(
        skills,
        agent_intent,
        user_message,
        source_hints,
        index,
        Some(scene),
    )
}

#[derive(Clone, Copy)]
struct SkillActivationBuildOptions<'a> {
    index: Option<&'a ActivationIndexMap>,
    legacy_scene_hint: Option<AiScene>,
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
    legacy_scene_hint: Option<AiScene>,
) -> SkillActivationPlanSummary {
    build_skill_activation_plan_for_task_inner(
        skills,
        agent_intent,
        user_message,
        source_hints,
        SkillActivationBuildOptions {
            index,
            legacy_scene_hint,
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
    legacy_scene_hint: Option<AiScene>,
    db: Option<&Database>,
) -> SkillActivationPlanSummary {
    build_skill_activation_plan_for_task_inner(
        skills,
        agent_intent,
        user_message,
        source_hints,
        SkillActivationBuildOptions {
            index,
            legacy_scene_hint,
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
    let vault_root = skills
        .iter()
        .find_map(|skill| Path::new(&skill.file_path).ancestors().nth(3))
        .map(Path::to_path_buf);
    let mut candidates: Vec<ScoredSkill<'_>> = Vec::new();
    for skill in skills.iter().filter(|skill| skill.enabled) {
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
    let mut requested_tools = Vec::new();
    let mut requested_capabilities: Vec<SkillRuntimeCapability> = Vec::new();
    let mut confirmation_required = Vec::new();
    let mut blocked = Vec::new();

    for scored in candidates.into_iter().take(3) {
        let skill = scored.skill;
        let reason = activation_reason(skill, agent_intent, user_message, source_hints)
            .map(|(_, reason)| reason)
            .unwrap_or_else(|| {
                if options.legacy_scene_hint.is_some() {
                    "legacy_scene_or_vector_match".into()
                } else {
                    "task_capability_or_vector_match".into()
                }
            });
        for tool in &skill.allowed_tools {
            if !requested_tools.contains(tool) {
                requested_tools.push(tool.clone());
            }
        }
        for capability in skill.requested_capabilities() {
            if !requested_capabilities.contains(&capability) {
                requested_capabilities.push(capability);
            }
        }
        for tool in confirmation_required_tools(&skill.allowed_tools) {
            if !confirmation_required.contains(&tool) {
                confirmation_required.push(tool);
            }
        }
        let blocked_caps = blocked_capabilities_for_skill(skill);
        blocked.extend(blocked_caps.clone());
        let workspace_status = vault_root
            .as_deref()
            .map(|vault| workspace_status_for_skill(vault, skill));
        let section_evaluation = options
            .enable_manifest_gating
            .then(|| evaluate_prompt_sections(skill, vault_root.as_deref(), options.db, None))
            .flatten();
        let injected_sections = section_evaluation
            .as_ref()
            .map(|evaluation| evaluation.injected_sections.clone())
            .unwrap_or_else(|| vec!["skill_overlay".into()]);
        let degraded_reasons = section_evaluation
            .as_ref()
            .map(|evaluation| evaluation.degraded_reasons.clone())
            .unwrap_or_default();
        activated.push(SkillActivationItemSummary {
            name: skill.name.clone(),
            scope: scope_wire(skill.scope),
            score: scored.score,
            match_reason: reason,
            injected_sections,
            degraded_reasons,
            requested_tools: skill.allowed_tools.clone(),
            requested_capabilities: skill.requested_capabilities(),
            confirmation_required_tools: confirmation_required_tools(&skill.allowed_tools),
            resources: build_resource_summaries(skill),
            blocked_capabilities: blocked_caps,
            compatibility_source: skill.compatibility_source(),
            workspace_root: workspace_status
                .as_ref()
                .map(|status| status.workspace_root.clone())
                .unwrap_or_else(|| workspace_root_relative(&skill.name)),
            workspace_ready: workspace_status
                .as_ref()
                .map(|status| status.workspace_ready)
                .unwrap_or(true),
            workspace_missing_items: workspace_status
                .map(|status| status.workspace_missing_items)
                .unwrap_or_default(),
        });
    }

    let manifest_degraded = activated
        .iter()
        .any(|skill| !skill.degraded_reasons.is_empty());
    SkillActivationPlanSummary {
        skill_overlay_summary: if activated.is_empty() {
            "No skills activated for this run.".into()
        } else {
            format!("{} skill(s) activated for skill_overlay.", activated.len())
        },
        activated_skills: activated,
        requested_tools,
        requested_capabilities,
        confirmation_required_tools: confirmation_required,
        degraded: !blocked.is_empty() || manifest_degraded,
        blocked_capabilities: blocked,
    }
}
/// Load enabled skills for prompt injection after metadata matching.
pub fn active_skills_for_prompt(
    vault: &Path,
    scene: AiScene,
    db: Option<&Database>,
    user_message: &str,
) -> AppResult<Vec<SkillEntry>> {
    active_skills_for_task_prompt(
        vault,
        intent_from_legacy_scene(scene),
        db,
        user_message,
        &[],
    )
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
            if skill.enabled {
                apply_runtime_prompt_sections(&mut skill, vault, db);
                out.push(skill);
            }
        }
    }
    Ok(out)
}

/// Union of allowed tools requested by active skills for a scene.
pub fn active_skill_allowed_tools(
    vault: &Path,
    scene: AiScene,
    db: Option<&Database>,
    user_message: &str,
) -> AppResult<Vec<String>> {
    active_skill_allowed_tools_for_task(
        vault,
        intent_from_legacy_scene(scene),
        db,
        user_message,
        &[],
    )
}

/// Union of allowed tools requested by active skills for a task.
pub fn active_skill_allowed_tools_for_task(
    vault: &Path,
    intent: AgentIntent,
    db: Option<&Database>,
    user_message: &str,
    source_hints: &[String],
) -> AppResult<Vec<String>> {
    let mut tools = Vec::new();
    for skill in active_skills_for_task_prompt(vault, intent, db, user_message, source_hints)? {
        for tool in skill.allowed_tools {
            if !tools.contains(&tool) {
                tools.push(tool);
            }
        }
    }
    Ok(tools)
}

/// Legacy wrapper for annotating list entries from a scene-shaped request.
pub fn enrich_list_with_scene(
    entries: Vec<SkillListEntry>,
    scene: AiScene,
    db: Option<&Database>,
) -> AppResult<Vec<SkillListEntry>> {
    enrich_list_with_task(
        entries,
        intent_from_legacy_scene(scene),
        scene.profile(),
        &[],
        db,
    )
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

#[cfg(test)]
mod phase4_tests {
    use std::collections::HashMap;

    use super::*;
    use crate::ai_types::SkillCapabilitySupportStatus;

    fn skill(name: &str) -> SkillEntry {
        SkillEntry {
            name: name.into(),
            description: format!("{name} research helper"),
            license: Some("AGPL-3.0".into()),
            compatibility: Some("hermes".into()),
            metadata: HashMap::new(),
            allowed_tools: vec![],
            content: "Use this skill carefully.".into(),
            scope: SkillScope::Vault,
            source_url: None,
            enabled: true,
            file_path: format!("/tmp/{name}/SKILL.md"),
            legacy_trigger: None,
        }
    }

    #[test]
    fn build_skill_activation_plan_prioritizes_explicit_skill_mention() {
        let skills = vec![skill("citation-helper"), skill("generic-helper")];

        let plan = build_skill_activation_plan(
            &skills,
            AiScene::KnowledgeLookup,
            AgentIntent::AskNotes,
            "Use citation-helper on this note",
            &[],
            None,
        );

        assert_eq!(plan.activated_skills[0].name, "citation-helper");
        assert_eq!(
            plan.activated_skills[0].match_reason,
            "explicit_skill_mention"
        );
    }

    #[test]
    fn build_skill_activation_plan_blocks_high_risk_runtime_capabilities() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "requested-capabilities".into(),
            serde_json::Value::String("skill.execute_script_sandboxed".into()),
        );
        let mut scripted = skill("scripted-helper");
        scripted.metadata = metadata;

        let plan = build_skill_activation_plan(
            &[scripted],
            AiScene::KnowledgeLookup,
            AgentIntent::AskNotes,
            "Use scripted-helper",
            &[],
            None,
        );

        assert!(plan.degraded);
        assert_eq!(
            plan.blocked_capabilities[0].status,
            SkillCapabilitySupportStatus::BlockedByPolicy
        );
        assert!(serde_json::to_string(&plan)
            .unwrap()
            .contains("skill.execute_script_sandboxed"));
        assert!(!serde_json::to_string(&plan).unwrap().contains("api_key"));
    }

    #[test]
    fn build_skill_activation_plan_with_runtime_skips_unavailable_runtime_sections() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        let skill_root = vault.join(".iris/skills/hybrid-helper");
        std::fs::create_dir_all(skill_root.join("sections")).unwrap();
        std::fs::write(
            skill_root.join("SKILL.md"),
            "---\nname: hybrid-helper\ndescription: hybrid helper\n---\n\nFallback",
        )
        .unwrap();
        std::fs::write(skill_root.join("sections/behavior.md"), "BEHAVIOR ONLY").unwrap();
        std::fs::write(skill_root.join("sections/web.md"), "WEB ONLY").unwrap();
        std::fs::write(
            skill_root.join("iris.skill.toml"),
            r#"
schema_version = "1"
name = "hybrid-helper"
kind = "hybrid"

[prompt]
default_sections = ["behavior", "web"]

[[prompt.sections]]
id = "behavior"
source = "sections/behavior.md"

[[prompt.sections]]
id = "web"
source = "sections/web.md"
requires_runtime = true

[[mcp.dependencies]]
profile_id = "anysearch"
required = true

[degradation]
when_runtime_missing = "partial"
message = "AnySearch runtime is unavailable."
"#,
        )
        .unwrap();

        let db = Database::open_in_memory().unwrap();
        let mut entry = skill("hybrid-helper");
        entry.file_path = skill_root.join("SKILL.md").to_string_lossy().into_owned();
        let plan = build_skill_activation_plan_for_task_with_runtime(
            &[entry],
            AgentIntent::AskNotes,
            "Use hybrid-helper",
            &[],
            None,
            Some(AiScene::KnowledgeLookup),
            Some(&db),
        );

        let activated = &plan.activated_skills[0];
        assert_eq!(activated.injected_sections, vec!["behavior".to_string()]);
        assert!(activated
            .degraded_reasons
            .iter()
            .any(|reason| reason.contains("web") && reason.contains("runtime")));
        assert!(plan.degraded);
    }

    #[test]
    fn build_skill_activation_plan_with_runtime_skips_unmapped_capability_sections() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        let skill_root = vault.join(".iris/skills/search-helper");
        std::fs::create_dir_all(skill_root.join("sections")).unwrap();
        std::fs::write(
            skill_root.join("SKILL.md"),
            "---\nname: search-helper\ndescription: search helper\n---\n\nFallback",
        )
        .unwrap();
        std::fs::write(skill_root.join("sections/behavior.md"), "BEHAVIOR ONLY").unwrap();
        std::fs::write(skill_root.join("sections/search.md"), "SEARCH ONLY").unwrap();
        std::fs::write(
            skill_root.join("iris.skill.toml"),
            r#"
schema_version = "1"
name = "search-helper"
kind = "hybrid"

[prompt]
default_sections = ["behavior", "search"]

[[prompt.sections]]
id = "behavior"
source = "sections/behavior.md"

[[prompt.sections]]
id = "search"
source = "sections/search.md"
requires_runtime = true
requires_capabilities = ["web.search"]

[capabilities]
requires = ["web.search"]

[[mcp.dependencies]]
profile_id = "search-profile"
required_capabilities = ["web.search"]
required = true
"#,
        )
        .unwrap();

        let db = Database::open_in_memory().unwrap();
        crate::ai_runtime::mcp_runtime_registry::upsert_server_catalog(
            &db,
            &crate::ai_runtime::mcp_runtime_registry::McpServerCatalogInput {
                id: "search-server".into(),
                display_name: "Search Server".into(),
                transport: "stdio".into(),
                command: Some("search-mcp".into()),
                args_json: "[]".into(),
                url: None,
                env_schema_json: "{}".into(),
                capability_tags_json: "[\"web.search\"]".into(),
                source: "test".into(),
            },
        )
        .unwrap();
        crate::ai_runtime::mcp_runtime_registry::upsert_runtime_profile(
            &db,
            &crate::ai_runtime::mcp_runtime_registry::McpRuntimeProfileInput {
                id: "search-profile".into(),
                server_id: "search-server".into(),
                vault_scope_hash: None,
                display_name: "Search profile".into(),
                enabled: true,
                transport_config_json: "{}".into(),
                env_bindings_json: "{}".into(),
                status: crate::ai_runtime::mcp_runtime_registry::McpRuntimeStatus::Ready,
                last_error: None,
            },
        )
        .unwrap();

        let mut entry = skill("search-helper");
        entry.file_path = skill_root.join("SKILL.md").to_string_lossy().into_owned();
        let plan = build_skill_activation_plan_for_task_with_runtime(
            &[entry],
            AgentIntent::AskNotes,
            "Use search-helper",
            &[],
            None,
            Some(AiScene::KnowledgeLookup),
            Some(&db),
        );

        let activated = &plan.activated_skills[0];
        assert_eq!(activated.injected_sections, vec!["behavior".to_string()]);
        assert!(activated
            .degraded_reasons
            .iter()
            .any(|reason| reason.contains("web.search")));
        assert!(plan.degraded);
    }
    #[test]
    fn active_skills_prompt_filters_unavailable_runtime_section_content() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        let skill_root = vault.join(".iris/skills/hybrid-helper");
        std::fs::create_dir_all(skill_root.join("sections")).unwrap();
        std::fs::write(
            skill_root.join("SKILL.md"),
            "---\nname: hybrid-helper\ndescription: hybrid helper\n---\n\nFallback",
        )
        .unwrap();
        std::fs::write(skill_root.join("sections/behavior.md"), "BEHAVIOR ONLY").unwrap();
        std::fs::write(skill_root.join("sections/web.md"), "WEB ONLY").unwrap();
        std::fs::write(
            skill_root.join("iris.skill.toml"),
            r#"
schema_version = "1"
name = "hybrid-helper"
kind = "hybrid"

[prompt]
default_sections = ["behavior", "web"]

[[prompt.sections]]
id = "behavior"
source = "sections/behavior.md"

[[prompt.sections]]
id = "web"
source = "sections/web.md"
requires_runtime = true

[[mcp.dependencies]]
profile_id = "anysearch"
required = true

[degradation]
when_runtime_missing = "partial"
"#,
        )
        .unwrap();

        let db = Database::open_in_memory().unwrap();
        let active = active_skills_for_task_prompt(
            &vault,
            AgentIntent::AskNotes,
            Some(&db),
            "Use hybrid-helper",
            &[],
        )
        .unwrap();

        assert_eq!(active.len(), 1);
        assert!(active[0].content.contains("BEHAVIOR ONLY"));
        assert!(!active[0].content.contains("WEB ONLY"));
    }
    #[test]
    fn build_skill_activation_plan_reports_resource_availability_and_truncation() {
        let dir = tempfile::tempdir().unwrap();
        let skill_root = dir.path().join(".iris/skills/resource-helper");
        std::fs::create_dir_all(skill_root.join("resources")).unwrap();
        std::fs::write(skill_root.join("resources/guide.md"), "a".repeat(25_000)).unwrap();

        let mut metadata = HashMap::new();
        metadata.insert(
            "required-resources".into(),
            serde_json::Value::String("resources/guide.md resources/missing.md".into()),
        );
        let mut entry = skill("resource-helper");
        entry.file_path = skill_root.join("SKILL.md").to_string_lossy().into_owned();
        entry.metadata = metadata;

        let plan = build_skill_activation_plan(
            &[entry],
            AiScene::KnowledgeLookup,
            AgentIntent::AskNotes,
            "Use resource-helper",
            &[],
            None,
        );

        let resources = &plan.activated_skills[0].resources;
        let guide = resources
            .iter()
            .find(|resource| resource.relative_path == "resources/guide.md")
            .expect("guide resource summary");
        assert!(guide.available);
        assert!(guide.truncated);
        assert!(guide.size_bytes.unwrap() > 24_000);

        let missing = resources
            .iter()
            .find(|resource| resource.relative_path == "resources/missing.md")
            .expect("missing resource summary");
        assert!(!missing.available);
        assert!(missing
            .reason
            .as_deref()
            .unwrap_or("")
            .contains("not found"));
    }

    #[test]
    fn build_skill_activation_plan_reports_workspace_status() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        let skill_root = vault.join(".iris/skills/workspace-helper");
        std::fs::create_dir_all(skill_root.join("resources")).unwrap();
        std::fs::create_dir_all(vault.join(".iris/skills-workspaces/workspace-helper/inputs"))
            .unwrap();
        std::fs::write(skill_root.join("resources/default-note.md"), "# Template").unwrap();

        let mut metadata = HashMap::new();
        metadata.insert(
            "iris-workspace".into(),
            serde_json::json!({
                "folders": ["inputs", "outputs"],
                "documents": [
                    {
                        "source": "resources/default-note.md",
                        "target": "README.md"
                    }
                ]
            }),
        );
        let mut entry = skill("workspace-helper");
        entry.file_path = skill_root.join("SKILL.md").to_string_lossy().into_owned();
        entry.metadata = metadata;

        let plan = build_skill_activation_plan(
            &[entry],
            AiScene::KnowledgeLookup,
            AgentIntent::AskNotes,
            "Use workspace-helper",
            &[],
            None,
        );

        let activated = &plan.activated_skills[0];
        assert_eq!(
            activated.workspace_root,
            ".iris/skills-workspaces/workspace-helper"
        );
        assert!(!activated.workspace_ready);
        assert!(activated
            .workspace_missing_items
            .contains(&"outputs/".to_string()));
        assert!(activated
            .workspace_missing_items
            .contains(&"README.md".to_string()));
    }
}
