use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ai_runtime::AiScene;
use crate::ai_types::{
    AgentIntent, SkillActivationItemSummary, SkillActivationPlanSummary,
    SkillResourceStatusSummary, SkillRuntimeCapability,
};
use crate::embedding::engine::{cosine_similarity, embed_text};
use crate::error::AppResult;
use crate::storage::db::Database;

use super::compatibility_impl::blocked_capabilities_for_skill;
use super::resources_impl::{ALLOWED_RESOURCE_DIRS, MAX_SKILL_RESOURCE_CHARS};
use super::validation_impl::confirmation_required_tools;
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

/// Filter and rank enabled skills by scene affinity with BM25-style scoring.
///
/// When `skill_activation_index` rows are supplied, keywords/description from the
/// index take precedence over file metadata for matching.
pub fn skills_for_scene(
    skills: &[SkillEntry],
    scene: AiScene,
    user_message: &str,
) -> Vec<SkillEntry> {
    let query = if user_message.trim().is_empty() {
        scene.profile()
    } else {
        user_message
    };
    rerank_skills_with_vectors(rank_skills_for_scene(skills, scene), query, None)
        .into_iter()
        .filter(|s| s.score >= 0.35)
        .take(3)
        .map(|s| s.skill.clone())
        .collect()
}

/// Scored version of `skills_for_scene` - returns scores for debugging/display.
pub fn rank_skills_for_scene<'a>(skills: &'a [SkillEntry], scene: AiScene) -> Vec<ScoredSkill<'a>> {
    rank_skills_for_scene_with_index(skills, scene, None)
}

/// Scored ranking with optional activation-index overlay.
pub fn rank_skills_for_scene_with_index<'a>(
    skills: &'a [SkillEntry],
    scene: AiScene,
    index: Option<&ActivationIndexMap>,
) -> Vec<ScoredSkill<'a>> {
    let scene_key = scene.profile();
    let scene_synonyms: Vec<&str> = match scene_key {
        "drafting_assist" => vec![
            "drafting",
            "writing",
            "compose",
            "editor",
            "drafting_assist",
        ],
        "research_synthesis" => vec![
            "research",
            "synthesis",
            "analysis",
            "evidence",
            "research_synthesis",
        ],
        "knowledge_lookup" => vec![
            "knowledge",
            "lookup",
            "search",
            "retrieve",
            "knowledge_lookup",
        ],
        "exemplar_learning" => vec![
            "exemplar",
            "learning",
            "template",
            "example",
            "exemplar_learning",
        ],
        _ => vec![scene_key],
    };

    let mut scored: Vec<ScoredSkill<'a>> = skills
        .iter()
        .filter(|s| s.enabled)
        .filter_map(|s| {
            let index_row = index.and_then(|m| m.get(&(s.name.clone(), s.scope)));
            let score = compute_skill_score(s, scene_key, &scene_synonyms, index_row);
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

/// BM25-style scoring for a single skill against a scene.
fn compute_skill_score(
    skill: &SkillEntry,
    scene_key: &str,
    synonyms: &[&str],
    index_row: Option<&SkillActivationIndexRow>,
) -> f64 {
    let mut score: f64 = 0.0;

    if skill.legacy_trigger.is_none() || skill.legacy_trigger.as_deref() == Some("") {
        score += 1.0;
    }

    if let Some(trigger) = &skill.legacy_trigger {
        let t = trigger.to_lowercase();
        if t.contains(scene_key) {
            score += 5.0;
        }
        for syn in synonyms {
            if t.contains(syn) {
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

    for syn in synonyms {
        let term = *syn;
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
            for syn in synonyms {
                if kw_lower.contains(syn) {
                    score += 2.0;
                }
            }
        }
    }

    score
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
    skill
        .required_resources()
        .into_iter()
        .map(|relative_path| build_resource_summary(skill_root, relative_path, "required"))
        .chain(
            skill
                .optional_resources()
                .into_iter()
                .map(|relative_path| build_resource_summary(skill_root, relative_path, "optional")),
        )
        .collect()
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
    let mut candidates: Vec<ScoredSkill<'_>> = Vec::new();
    for skill in skills.iter().filter(|skill| skill.enabled) {
        if let Some((score, _reason)) =
            activation_reason(skill, agent_intent, user_message, source_hints)
        {
            candidates.push(ScoredSkill { skill, score });
        }
    }

    let query = if user_message.trim().is_empty() {
        scene.profile()
    } else {
        user_message
    };
    let ranked = rerank_skills_with_vectors(
        rank_skills_for_scene_with_index(skills, scene, index),
        query,
        index,
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
            .unwrap_or_else(|| "legacy_scene_or_vector_match".into());
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
        activated.push(SkillActivationItemSummary {
            name: skill.name.clone(),
            scope: scope_wire(skill.scope),
            score: scored.score,
            match_reason: reason,
            injected_sections: vec!["skill_overlay".into()],
            requested_tools: skill.allowed_tools.clone(),
            requested_capabilities: skill.requested_capabilities(),
            confirmation_required_tools: confirmation_required_tools(&skill.allowed_tools),
            resources: build_resource_summaries(skill),
            blocked_capabilities: blocked_caps,
            compatibility_source: skill.compatibility_source(),
        });
    }

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
        degraded: !blocked.is_empty(),
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
    let query = if user_message.trim().is_empty() {
        scene.profile()
    } else {
        user_message
    };
    let ranked = rerank_skills_with_vectors(
        rank_skills_for_scene_with_index(&metadata, scene, index_ref),
        query,
        index_ref,
    );
    let mut out = Vec::new();
    for scored in ranked {
        let path = PathBuf::from(&scored.skill.file_path);
        if let Ok(mut skill) = load_skill(&path, scored.skill.scope) {
            skill.enabled = scored.skill.enabled;
            if skill.enabled {
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
    let mut tools = Vec::new();
    for skill in active_skills_for_prompt(vault, scene, db, user_message)? {
        for tool in skill.allowed_tools {
            if !tools.contains(&tool) {
                tools.push(tool);
            }
        }
    }
    Ok(tools)
}

/// Annotate list entries with scene affinity when a scene is provided.
pub fn enrich_list_with_scene(
    mut entries: Vec<SkillListEntry>,
    scene: AiScene,
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
        rank_skills_for_scene_with_index(&skills, scene, index_ref),
        scene.profile(),
        index_ref,
    );
    let score_map: HashMap<(String, SkillScope), f64> = ranked
        .iter()
        .map(|s| ((s.skill.name.clone(), s.skill.scope), s.score))
        .collect();

    for entry in &mut entries {
        let key = (entry.skill.name.clone(), entry.skill.scope);
        entry.scene_active = Some(score_map.contains_key(&key));
        entry.scene_score = score_map.get(&key).copied();
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
}
