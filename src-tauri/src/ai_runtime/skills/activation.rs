use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ai_runtime::AiScene;
use crate::embedding::engine::{cosine_similarity, embed_text};
use crate::error::AppResult;
use crate::storage::db::Database;

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
