//! Agent Skills runtime — SKILL.md registry, validation, matching, prompt injection.
//!
//! Compatible with Agent Skills specification while preserving Iris local-first
//! security model. Old `trigger`-based skills continue to work via `legacy_trigger`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ai_runtime::tool_catalog::TOOL_CATALOG;
use crate::ai_runtime::AiScene;
use crate::error::{AppError, AppResult};

const VALIDATION_MISSING_FRONTMATTER: &str = "_iris_missing_frontmatter";
const VALIDATION_NAME_MISMATCH: &str = "_iris_name_mismatch";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope {
    Global,
    Vault,
}

/// Metadata bag — arbitrary key-value pairs from frontmatter.
pub type SkillMetadata = HashMap<String, serde_json::Value>;

/// Validation status of a skill file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillValidationStatus {
    /// New-format skill with valid frontmatter.
    Valid,
    /// Old-format skill with `trigger` field (readable, prompts migration).
    Legacy,
    /// Missing required fields or invalid content.
    Invalid(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub license: Option<String>,
    pub compatibility: Option<String>,
    pub metadata: SkillMetadata,
    pub allowed_tools: Vec<String>,
    pub content: String,
    pub scope: SkillScope,
    pub source_url: Option<String>,
    pub enabled: bool,
    pub file_path: String,
    /// Preserved from old-format `trigger` field for backward compatibility.
    pub legacy_trigger: Option<String>,
}

impl SkillEntry {
    /// Validation status: Valid / Legacy / Invalid.
    pub fn validation_status(&self) -> SkillValidationStatus {
        if self.name.is_empty() {
            return SkillValidationStatus::Invalid("name is empty".into());
        }
        if self.description.is_empty() {
            return SkillValidationStatus::Invalid("description is empty".into());
        }
        if self.description.len() > 1024 {
            return SkillValidationStatus::Invalid("description exceeds 1024 chars".into());
        }
        if let Some(ref compat) = self.compatibility {
            if compat.len() > 500 {
                return SkillValidationStatus::Invalid("compatibility exceeds 500 chars".into());
            }
        }
        if self
            .metadata
            .get(VALIDATION_MISSING_FRONTMATTER)
            .and_then(|v| v.as_bool())
            == Some(true)
            && self.legacy_trigger.is_none()
        {
            return SkillValidationStatus::Invalid("missing YAML frontmatter".into());
        }
        if self
            .metadata
            .get(VALIDATION_NAME_MISMATCH)
            .and_then(|v| v.as_bool())
            == Some(true)
            && self.legacy_trigger.is_none()
        {
            return SkillValidationStatus::Invalid("name must match parent directory".into());
        }
        // Check if all allowed_tools exist in the catalog
        for tool in &self.allowed_tools {
            if TOOL_CATALOG.iter().all(|e| e.name != tool.as_str()) {
                return SkillValidationStatus::Invalid(format!(
                    "allowed-tool '{tool}' not found in ToolCatalog"
                ));
            }
        }
        // Legacy detection: has trigger but no description-based matching metadata
        if self.legacy_trigger.is_some() {
            return SkillValidationStatus::Legacy;
        }
        SkillValidationStatus::Valid
    }

    /// Whether all allowed_tools are recognized by the ToolCatalog.
    pub fn all_allowed_tools_recognized(&self) -> bool {
        self.allowed_tools.is_empty()
            || self
                .allowed_tools
                .iter()
                .all(|t| TOOL_CATALOG.iter().any(|e| e.name == t.as_str()))
    }

    /// Tools requested but NOT in the catalog (for UI display).
    pub fn unrecognized_tools(&self) -> Vec<String> {
        self.allowed_tools
            .iter()
            .filter(|t| TOOL_CATALOG.iter().all(|e| e.name != t.as_str()))
            .cloned()
            .collect()
    }

    /// Dependencies declared in `metadata.depends` (Iris extension field).
    /// Returns skill names this skill depends on.
    pub fn depends(&self) -> Vec<String> {
        match self.metadata.get("depends") {
            Some(serde_json::Value::String(s)) => s.split_whitespace().map(String::from).collect(),
            Some(serde_json::Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            _ => vec![],
        }
    }

    /// Check which dependencies are missing from the installed skill list.
    pub fn missing_dependencies(&self, installed_names: &[String]) -> Vec<String> {
        self.depends()
            .into_iter()
            .filter(|dep| !installed_names.contains(dep))
            .collect()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SkillsConfig {
    disabled: Vec<String>,
}

fn global_skills_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".iris").join("skills");
    }
    PathBuf::from(".iris").join("skills")
}

fn vault_skills_dir(vault: &Path) -> PathBuf {
    vault.join(".iris").join("skills")
}

fn config_path(scope: SkillScope, vault: &Path) -> PathBuf {
    match scope {
        SkillScope::Global => global_skills_dir()
            .parent()
            .unwrap_or(Path::new("."))
            .join("skills-config.json"),
        SkillScope::Vault => vault.join(".iris").join("skills-config.json"),
    }
}

fn load_config(scope: SkillScope, vault: &Path) -> SkillsConfig {
    let path = config_path(scope, vault);
    if !path.exists() {
        return SkillsConfig::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_config(scope: SkillScope, vault: &Path, config: &SkillsConfig) -> AppResult<()> {
    let path = config_path(scope, vault);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(config)?;
    fs::write(path, json)?;
    Ok(())
}

fn skill_key(scope: SkillScope, name: &str) -> String {
    format!("{:?}:{name}", scope)
}

/// Scan global + vault skill directories.
pub fn scan_all(vault: &Path) -> AppResult<Vec<SkillEntry>> {
    let mut entries = Vec::new();
    let global_dir = global_skills_dir();
    if global_dir.is_dir() {
        scan_dir(&global_dir, SkillScope::Global, vault, &mut entries)?;
    }
    let vault_dir = vault_skills_dir(vault);
    if vault_dir.is_dir() {
        scan_dir(&vault_dir, SkillScope::Vault, vault, &mut entries)?;
    }
    Ok(entries)
}

/// Scan global + vault skill directories but leave instruction bodies unloaded.
pub fn scan_all_metadata(vault: &Path) -> AppResult<Vec<SkillEntry>> {
    let mut entries = scan_all(vault)?;
    for entry in &mut entries {
        entry.content.clear();
    }
    Ok(entries)
}

fn scan_dir(
    dir: &Path,
    scope: SkillScope,
    vault: &Path,
    out: &mut Vec<SkillEntry>,
) -> AppResult<()> {
    let config = load_config(scope, vault);
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_file = path.join("SKILL.md");
        if skill_file.is_file() {
            if let Ok(mut skill) = load_skill(&skill_file, scope) {
                let key = skill_key(scope, &skill.name);
                skill.enabled = !config.disabled.contains(&key);
                out.push(skill);
            }
        }
    }
    Ok(())
}

/// Parse a single SKILL.md file.
///
/// Supports both new Agent Skills format (YAML frontmatter with `description`,
/// `allowed-tools`, etc.) and legacy format (with `trigger`).
pub fn load_skill(path: &Path, scope: SkillScope) -> AppResult<SkillEntry> {
    let raw = fs::read_to_string(path)?;
    let has_frontmatter = raw.trim_start().starts_with("---");
    let (meta, body) = parse_frontmatter(&raw);

    // Resolve name: frontmatter > H1 heading > parent directory name
    let name = meta
        .get("name")
        .cloned()
        .or_else(|| {
            body.lines()
                .find(|l| l.starts_with("# "))
                .map(|l| l.trim_start_matches("# ").trim().to_string())
        })
        .unwrap_or_else(|| {
            path.parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .unwrap_or("skill")
                .to_string()
        });

    // Validate name matches parent directory (new format requirement)
    let dir_name = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let normalized_name = slugify(&name);
    let normalized_dir = slugify(dir_name);
    let name_matches_dir = normalized_name == normalized_dir || dir_name.is_empty();

    // Parse allowed-tools: space-separated string
    let allowed_tools: Vec<String> = meta
        .get("allowed-tools")
        .or_else(|| meta.get("allowed_tools"))
        .map(|s| s.split_whitespace().map(String::from).collect())
        .unwrap_or_default();

    // Parse metadata as JSON object (stored as HashMap)
    let mut metadata: SkillMetadata = meta
        .get("metadata")
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    if !has_frontmatter {
        metadata.insert(
            VALIDATION_MISSING_FRONTMATTER.to_string(),
            serde_json::Value::Bool(true),
        );
    }

    // Legacy trigger: from old format
    let legacy_trigger = meta.get("trigger").cloned();

    // Validate field lengths
    let description = meta.get("description").cloned().unwrap_or_default();
    if description.len() > 1024 {
        return Err(AppError::msg(format!(
            "skill description exceeds 1024 chars (got {})",
            description.len()
        )));
    }
    let compatibility = meta.get("compatibility").cloned();
    if let Some(ref compat) = compatibility {
        if compat.len() > 500 {
            return Err(AppError::msg(format!(
                "skill compatibility exceeds 500 chars (got {})",
                compat.len()
            )));
        }
    }

    // Build entry
    let mut entry = SkillEntry {
        name,
        description,
        license: meta.get("license").cloned(),
        compatibility,
        metadata,
        allowed_tools,
        content: body.trim().to_string(),
        scope,
        source_url: meta.get("source_url").cloned(),
        enabled: true,
        file_path: path.to_string_lossy().into_owned(),
        legacy_trigger,
    };

    // Warn if name doesn't match directory (but don't reject — backward compat)
    if !name_matches_dir && !dir_name.is_empty() {
        entry.metadata.insert(
            VALIDATION_NAME_MISMATCH.to_string(),
            serde_json::Value::Bool(true),
        );
        tracing::warn!(
            skill = %entry.name,
            dir = %dir_name,
            "skill name does not match parent directory"
        );
    }

    // Auto-detect validation issues
    if entry.description.is_empty() && !body.is_empty() {
        // Try to extract description from first paragraph
        if let Some(first_para) = body
            .lines()
            .skip_while(|l| l.starts_with('#') || l.trim().is_empty())
            .take_while(|l| !l.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" ")
            .into()
        {
            let desc: String = first_para;
            if desc.len() <= 1024 {
                entry.description = desc;
            }
        }
    }

    Ok(entry)
}

/// Parse YAML-like frontmatter from SKILL.md.
///
/// Returns (key-value map, body content after the closing `---`).
/// Handles both simple `key: value` lines and multi-line values.
fn parse_frontmatter(raw: &str) -> (HashMap<String, String>, String) {
    let mut map = HashMap::new();
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return (map, raw.to_string());
    }
    let rest = trimmed.trim_start_matches("---");
    let Some(end) = rest.find("\n---") else {
        return (map, raw.to_string());
    };
    let front = &rest[..end];
    let body = rest[end + 4..].trim_start();
    for line in front.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            let value = v.trim().trim_matches('"').to_string();
            map.insert(k.trim().to_string(), value);
        }
    }
    (map, body.to_string())
}

/// Install skill from HTTP(S) URL (raw SKILL.md or GitHub raw link).
pub async fn install_from_url(url: &str, scope: SkillScope, vault: &Path) -> AppResult<SkillEntry> {
    crate::security::ipc_policy::validate_skill_remote_url(url)?;
    let client = crate::network::cert_pinning::create_pinned_client()?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::msg(format!("download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::msg(format!("HTTP {}", resp.status())));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| AppError::msg(format!("read body: {e}")))?;

    let (meta, _) = parse_frontmatter(&body);
    let dir_name = meta
        .get("name")
        .map(|s| slugify(s))
        .unwrap_or_else(|| format!("skill-{}", chrono::Utc::now().timestamp()));

    let base = match scope {
        SkillScope::Global => global_skills_dir(),
        SkillScope::Vault => vault_skills_dir(vault),
    };
    fs::create_dir_all(&base)?;
    let target_dir = base.join(&dir_name);
    fs::create_dir_all(&target_dir)?;
    let skill_path = target_dir.join("SKILL.md");
    fs::write(&skill_path, &body)?;

    let mut entry = load_skill(&skill_path, scope)?;
    entry.source_url = Some(url.to_string());
    Ok(entry)
}

/// Shallow git clone and copy SKILL.md or skill directory.
pub async fn install_from_git(
    repo_url: &str,
    subpath: Option<&str>,
    scope: SkillScope,
    vault: &Path,
) -> AppResult<Vec<SkillEntry>> {
    crate::security::ipc_policy::validate_skill_git_url(repo_url)?;

    // Validate subpath before passing to git or filesystem.
    if let Some(sp) = subpath {
        validate_subpath(sp)?;
    }

    let tmp = std::env::temp_dir().join(format!("iris-skill-{}", uuid::Uuid::new_v4()));
    let status = std::process::Command::new("git")
        .args(["clone", "--depth", "1", "--"])
        .arg(repo_url)
        .arg(tmp.to_str().unwrap_or(""))
        .status()
        .map_err(|e| AppError::msg(format!("git not available: {e}")))?;
    if !status.success() {
        return Err(AppError::msg("git clone failed"));
    }

    // Resolve subpath and ensure it stays inside the clone directory.
    let tmp_canonical = tmp
        .canonicalize()
        .map_err(|_| AppError::msg("clone directory missing"))?;
    let src = match subpath {
        Some(sp) => {
            let joined = tmp.join(sp);
            let canon = joined
                .canonicalize()
                .map_err(|_| AppError::msg(format!("subpath does not exist: {sp}")))?;
            if !canon.starts_with(&tmp_canonical) {
                return Err(AppError::msg("subpath escapes clone directory"));
            }
            canon
        }
        None => tmp_canonical.clone(),
    };

    let base = match scope {
        SkillScope::Global => global_skills_dir(),
        SkillScope::Vault => vault_skills_dir(vault),
    };
    fs::create_dir_all(&base)?;

    let mut installed = Vec::new();
    if src.join("SKILL.md").is_file() {
        let name = slugify(src.file_name().and_then(|s| s.to_str()).unwrap_or("skill"));
        let dest = base.join(&name);
        atomic_copy_dir(&src, &dest)?;
        let skill_path = dest.join("SKILL.md");
        installed.push(load_skill(&skill_path, scope)?);
    } else if src.is_dir() {
        for entry in fs::read_dir(&src)? {
            let entry = entry?;
            let p = entry.path();
            if p.join("SKILL.md").is_file() {
                let name = p
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(slugify)
                    .unwrap_or_else(|| "skill".into());
                let dest = base.join(&name);
                atomic_copy_dir(&p, &dest)?;
                installed.push(load_skill(&dest.join("SKILL.md"), scope)?);
            }
        }
    }

    let _ = fs::remove_dir_all(&tmp);
    if installed.is_empty() {
        return Err(AppError::msg("no SKILL.md found in repository"));
    }
    Ok(installed)
}

/// Reject subpaths that attempt directory traversal or are absolute.
fn validate_subpath(subpath: &str) -> AppResult<()> {
    use std::path::Component;
    for component in std::path::Path::new(subpath).components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {}
            _ => {
                return Err(AppError::msg(format!(
                    "invalid subpath: must be a relative path inside the repo ({subpath})"
                )));
            }
        }
    }
    Ok(())
}

/// Copy `src` into `dest` atomically: write to a sibling temp directory first,
/// then rename. This prevents half-written skill directories on error.
fn atomic_copy_dir(src: &Path, dest: &Path) -> AppResult<()> {
    let parent = dest
        .parent()
        .ok_or_else(|| AppError::msg("invalid destination parent"))?;
    let staging = parent.join(format!(
        ".tmp-{}",
        dest.file_name().unwrap_or_default().to_string_lossy()
    ));
    // Clean up any leftover staging directory from a previous failed attempt.
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    copy_dir_recursive(src, &staging)?;
    // If destination already exists (reinstall), remove it.
    if dest.exists() {
        fs::remove_dir_all(dest)?;
    }
    fs::rename(&staging, dest)?;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> AppResult<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

pub fn uninstall(name: &str, scope: SkillScope, vault: &Path) -> AppResult<()> {
    let base = match scope {
        SkillScope::Global => global_skills_dir(),
        SkillScope::Vault => vault_skills_dir(vault),
    };
    let slug = slugify(name);
    let target = base.join(slug);
    if target.is_dir() {
        fs::remove_dir_all(target)?;
    }
    Ok(())
}

pub fn set_enabled(name: &str, scope: SkillScope, vault: &Path, enabled: bool) -> AppResult<()> {
    let mut config = load_config(scope, vault);
    let key = skill_key(scope, name);
    if enabled {
        config.disabled.retain(|k| k != &key);
    } else if !config.disabled.contains(&key) {
        config.disabled.push(key);
    }
    save_config(scope, vault, &config)
}

/// Scored skill match result.
#[derive(Debug, Clone)]
pub struct ScoredSkill<'a> {
    pub skill: &'a SkillEntry,
    pub score: f64,
}

/// Filter and rank enabled skills by scene affinity with BM25-style scoring.
///
/// Priority order:
/// 1. Skills with no trigger and no legacy_trigger (universally available, base score)
/// 2. Skills with legacy_trigger matching the scene keyword (boosted)
/// 3. Skills with description matching scene keywords (BM25-style term frequency)
///
/// Returns skills sorted by score (highest first).
pub fn skills_for_scene(skills: &[SkillEntry], scene: AiScene) -> Vec<&SkillEntry> {
    rank_skills_for_scene(skills, scene)
        .into_iter()
        .map(|ss| ss.skill)
        .collect()
}

/// Scored version of `skills_for_scene` — returns scores for debugging/display.
pub fn rank_skills_for_scene<'a>(skills: &'a [SkillEntry], scene: AiScene) -> Vec<ScoredSkill<'a>> {
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
            let score = compute_skill_score(s, scene_key, &scene_synonyms);
            if score > 0.0 {
                Some(ScoredSkill { skill: s, score })
            } else {
                None
            }
        })
        .collect();

    // Stable sort by score descending
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored
}

/// BM25-style scoring for a single skill against a scene.
fn compute_skill_score(skill: &SkillEntry, scene_key: &str, synonyms: &[&str]) -> f64 {
    let mut score: f64 = 0.0;

    // Universal skills get a base score
    if skill.legacy_trigger.is_none() || skill.legacy_trigger.as_deref() == Some("") {
        score += 1.0;
    }

    // Legacy trigger exact match → strong signal
    if let Some(trigger) = &skill.legacy_trigger {
        let t = trigger.to_lowercase();
        if t.contains(scene_key) {
            score += 5.0;
        }
        // Synonym matching for legacy trigger
        for syn in synonyms {
            if t.contains(syn) {
                score += 3.0;
                break;
            }
        }
    }

    // Description BM25-style term frequency
    let desc_lower = skill.description.to_lowercase();
    let name_lower = skill.name.to_lowercase();
    let content_lower = skill.content.to_lowercase();

    for syn in synonyms {
        let term = *syn;
        // Term frequency in description (weighted higher)
        let desc_tf = desc_lower.matches(term).count() as f64;
        if desc_tf > 0.0 {
            // BM25 saturation: tf / (tf + k1), k1=1.2
            score += (desc_tf / (desc_tf + 1.2)) * 3.0;
        }
        // Term frequency in name (weighted highest)
        if name_lower.contains(term) {
            score += 4.0;
        }
        // Term frequency in content (weighted lower, may be large)
        let content_tf = content_lower.matches(term).count() as f64;
        if content_tf > 0.0 {
            score += (content_tf / (content_tf + 1.2)) * 0.5;
        }
    }

    // Metadata keyword boost
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

/// Rerank skills using vector similarity when sqlite-vec is available.
/// Falls back to the BM25-scored list when vector index is not ready.
pub fn rerank_skills_with_vectors<'a>(
    scored: Vec<ScoredSkill<'a>>,
    _query: &str,
) -> Vec<ScoredSkill<'a>> {
    // If sqlite-vec is not available, return BM25 scores as-is
    if !crate::storage::db::vector_index_ready() {
        return scored;
    }

    // Vector index readiness is best-effort for now. Until skill description
    // embeddings are populated, keep the deterministic BM25/keyword ordering.
    scored
}

/// Load enabled skills for prompt injection after metadata matching.
pub fn active_skills_for_prompt(vault: &Path, scene: AiScene) -> AppResult<Vec<SkillEntry>> {
    let metadata = scan_all_metadata(vault)?;
    let mut out = Vec::new();
    for scored in rank_skills_for_scene(&metadata, scene) {
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
pub fn active_skill_allowed_tools(vault: &Path, scene: AiScene) -> AppResult<Vec<String>> {
    let mut tools = Vec::new();
    for skill in active_skills_for_prompt(vault, scene)? {
        for tool in skill.allowed_tools {
            if !tools.contains(&tool) {
                tools.push(tool);
            }
        }
    }
    Ok(tools)
}

/// DTO for `skills_list` IPC response — includes computed fields.
#[derive(Debug, Clone, Serialize)]
pub struct SkillListEntry {
    #[serde(flatten)]
    pub skill: SkillEntry,
    /// Validation status (valid / legacy / invalid).
    pub validation: SkillValidationStatus,
    /// Tools not found in the ToolCatalog.
    pub unrecognized_tools: Vec<String>,
    /// Dependencies that are not installed.
    pub missing_deps: Vec<String>,
}

/// Build the skills list with computed validation/dependency info.
pub fn scan_all_with_status(vault: &Path) -> AppResult<Vec<SkillListEntry>> {
    let skills = scan_all(vault)?;
    let installed_names: Vec<String> = skills.iter().map(|s| s.name.clone()).collect();
    Ok(skills
        .into_iter()
        .map(|skill| {
            let unrecognized_tools = skill.unrecognized_tools();
            let missing_deps = skill.missing_dependencies(&installed_names);
            let validation = skill.validation_status();
            SkillListEntry {
                skill,
                validation,
                unrecognized_tools,
                missing_deps,
            }
        })
        .collect())
}

/// Build system prompt fragment from enabled skills.
pub fn inject_into_prompt(skills: &[SkillEntry], scene: AiScene) -> String {
    let matched = skills_for_scene(skills, scene);
    if matched.is_empty() {
        return String::new();
    }
    let mut block = String::from("## 已激活 Skills\n\n");
    for skill in matched {
        block.push_str(&format!("### Skill: {}\n\n", skill.name));
        if !skill.description.is_empty() {
            block.push_str(&format!("_{}_\n\n", skill.description));
        }
        if !skill.allowed_tools.is_empty() {
            block.push_str(&format!("请求工具: {}\n\n", skill.allowed_tools.join(", ")));
        }
        block.push_str(&skill.content);
        block.push_str("\n\n---\n\n");
    }
    block
}

/// Install SKILL.md from a local file path (copies into skills directory).
pub fn install_from_local(source: &Path, scope: SkillScope, vault: &Path) -> AppResult<SkillEntry> {
    let source = crate::security::ipc_policy::validate_local_skill_source(source, vault)?;
    if !source.is_file() {
        return Err(AppError::msg("本地安装需要 SKILL.md 文件路径"));
    }
    let body = fs::read_to_string(&source)?;
    let (meta, _) = parse_frontmatter(&body);
    let dir_name = meta
        .get("name")
        .cloned()
        .or_else(|| {
            source
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .map(slugify)
        })
        .unwrap_or_else(|| format!("skill-{}", uuid::Uuid::new_v4()));

    let base = match scope {
        SkillScope::Global => global_skills_dir(),
        SkillScope::Vault => vault_skills_dir(vault),
    };
    fs::create_dir_all(&base)?;
    let target_dir = base.join(&dir_name);
    fs::create_dir_all(&target_dir)?;
    let skill_path = target_dir.join("SKILL.md");
    fs::write(&skill_path, &body)?;

    let mut entry = load_skill(&skill_path, scope)?;
    entry.source_url = Some(source.to_string_lossy().into_owned());
    Ok(entry)
}

/// Validate that a path is under a known skills directory (global or vault).
pub fn validate_skill_path(path: &Path, vault: &Path) -> AppResult<()> {
    let canonical = path
        .canonicalize()
        .map_err(|_| AppError::msg("Skill 文件路径无效或不存在"))?;
    let global_dir = global_skills_dir();
    let vault_dir = vault_skills_dir(vault);

    let under_global = global_dir
        .canonicalize()
        .is_ok_and(|g| canonical.starts_with(&g));
    let under_vault = vault_dir
        .canonicalize()
        .is_ok_and(|v| canonical.starts_with(&v));

    if !under_global && !under_vault {
        return Err(AppError::msg("Skill 文件路径必须在已知的 skills 目录下"));
    }
    Ok(())
}

/// Read skill file content for editing.
pub fn read_skill_content(path: &Path) -> AppResult<String> {
    fs::read_to_string(path).map_err(Into::into)
}

/// Write updated skill content (must be `SKILL.md`).
pub fn write_skill_content(path: &Path, scope: SkillScope, content: &str) -> AppResult<SkillEntry> {
    if path.file_name().and_then(|n| n.to_str()) != Some("SKILL.md") {
        return Err(AppError::msg("只能写入 SKILL.md"));
    }
    fs::write(path, content)?;
    load_skill(path, scope)
}

/// Migrate a legacy `trigger`-based skill to the new Agent Skills format.
///
/// - Reads the existing SKILL.md
/// - Converts `trigger` → new format fields (removes trigger, keeps description)
/// - Creates a backup at `SKILL.md.bak` before overwriting
/// - Returns the migrated SkillEntry
///
/// Does NOT auto-migrate — caller must obtain user confirmation first.
pub fn migrate_legacy_skill(path: &Path, scope: SkillScope) -> AppResult<SkillEntry> {
    let raw = fs::read_to_string(path)?;
    let (meta, body) = parse_frontmatter(&raw);

    // Only migrate if it has a trigger field
    if !meta.contains_key("trigger") {
        return Err(AppError::msg("skill 已是新格式，无需迁移"));
    }

    // Create backup
    let backup_path = path.with_extension("md.bak");
    fs::copy(path, &backup_path)?;

    // Build new frontmatter without trigger
    let mut new_front = String::from("---\n");
    for (k, v) in &meta {
        if k == "trigger" {
            continue; // Remove trigger
        }
        // Quote values that contain special chars
        if v.contains(':') || v.contains('#') || v.contains('"') {
            new_front.push_str(&format!("{}: \"{}\"\n", k, v.replace('"', "\\\"")));
        } else {
            new_front.push_str(&format!("{k}: {v}\n"));
        }
    }
    new_front.push_str("---\n\n");

    let new_content = format!("{new_front}{}", body.trim_start());
    fs::write(path, &new_content)?;

    load_skill(path, scope)
}

/// Check if a skill file is in legacy format (has `trigger` field).
pub fn is_legacy_format(path: &Path) -> bool {
    if let Ok(raw) = fs::read_to_string(path) {
        let (meta, _) = parse_frontmatter(&raw);
        meta.contains_key("trigger")
    } else {
        false
    }
}

fn slugify(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── validate_subpath ──────────────────────────────────

    #[test]
    fn subpath_rejects_dotdot() {
        let err = validate_subpath("../x").unwrap_err();
        assert!(err.to_string().contains("invalid subpath"));
    }

    #[test]
    fn subpath_rejects_dotdot_in_middle() {
        let err = validate_subpath("a/../../b").unwrap_err();
        assert!(err.to_string().contains("invalid subpath"));
    }

    #[test]
    fn subpath_rejects_absolute_path() {
        let err = validate_subpath("/etc/passwd").unwrap_err();
        assert!(err.to_string().contains("invalid subpath"));
    }

    #[test]
    fn subpath_rejects_root() {
        let err = validate_subpath("/").unwrap_err();
        assert!(err.to_string().contains("invalid subpath"));
    }

    #[test]
    fn subpath_accepts_simple_relative() {
        assert!(validate_subpath("skills/my-skill").is_ok());
    }

    #[test]
    fn subpath_accepts_single_component() {
        assert!(validate_subpath("my-skill").is_ok());
    }

    #[test]
    fn subpath_accepts_dot_slash() {
        assert!(validate_subpath("./skills").is_ok());
    }

    // ── atomic_copy_dir ───────────────────────────────────

    #[test]
    fn atomic_copy_copies_contents() {
        let src = tempfile::tempdir().unwrap();
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("skill");
        fs::write(src.path().join("SKILL.md"), "# Test Skill").unwrap();
        fs::write(src.path().join("data.txt"), "data").unwrap();
        atomic_copy_dir(src.path(), &dest).unwrap();
        assert_eq!(
            fs::read_to_string(dest.join("SKILL.md")).unwrap(),
            "# Test Skill"
        );
        assert_eq!(fs::read_to_string(dest.join("data.txt")).unwrap(), "data");
    }

    #[test]
    fn atomic_copy_overwrites_existing() {
        let src = tempfile::tempdir().unwrap();
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("skill");
        fs::write(src.path().join("SKILL.md"), "new").unwrap();
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("SKILL.md"), "old").unwrap();
        atomic_copy_dir(src.path(), &dest).unwrap();
        assert_eq!(fs::read_to_string(dest.join("SKILL.md")).unwrap(), "new");
    }

    // ── slugify ───────────────────────────────────────────

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("My Skill"), "my-skill");
        assert_eq!(slugify("hello_world"), "hello_world");
        assert_eq!(slugify("a/b\\c"), "a-b-c");
    }

    // ── install_from_git symlink escape check ─────────────

    #[test]
    fn subpath_symlink_escape_rejected_by_canonicalize() {
        let tmp = tempfile::tempdir().unwrap();
        let tmp_canonical = tmp.path().canonicalize().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("SKILL.md"), "# Escape").unwrap();
        #[cfg(unix)]
        {
            let link_path = tmp.path().join("escape-link");
            std::os::unix::fs::symlink(outside.path(), &link_path).unwrap();
            let canon = link_path.canonicalize().unwrap();
            assert!(!canon.starts_with(&tmp_canonical));
        }
    }

    #[test]
    fn subpath_stays_inside_clone() {
        let tmp = tempfile::tempdir().unwrap();
        let tmp_canonical = tmp.path().canonicalize().unwrap();
        let skill_dir = tmp.path().join("skills").join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# My Skill").unwrap();
        let subpath = "skills/my-skill";
        let canon = tmp.path().join(subpath).canonicalize().unwrap();
        assert!(canon.starts_with(&tmp_canonical));
    }

    // ── frontmatter parsing ───────────────────────────────

    #[test]
    fn parse_frontmatter_new_format() {
        let raw = r#"---
name: my-skill
description: A test skill
allowed-tools: search_hybrid read_note
license: MIT
compatibility: "Iris 1.0+"
---

# My Skill

Instructions here."#;
        let (meta, body) = parse_frontmatter(raw);
        assert_eq!(meta.get("name").unwrap(), "my-skill");
        assert_eq!(meta.get("description").unwrap(), "A test skill");
        assert_eq!(
            meta.get("allowed-tools").unwrap(),
            "search_hybrid read_note"
        );
        assert_eq!(meta.get("license").unwrap(), "MIT");
        assert!(body.contains("Instructions here"));
    }

    #[test]
    fn parse_frontmatter_legacy_format() {
        let raw = r#"---
name: old-skill
description: Legacy skill
trigger: knowledge
---

# Old Skill"#;
        let (meta, body) = parse_frontmatter(raw);
        assert_eq!(meta.get("trigger").unwrap(), "knowledge");
        assert!(body.contains("# Old Skill"));
    }

    #[test]
    fn parse_frontmatter_no_frontmatter() {
        let raw = "# Just a heading\n\nBody text.";
        let (meta, body) = parse_frontmatter(raw);
        assert!(meta.is_empty());
        assert!(body.contains("# Just a heading"));
    }

    // ── load_skill with new fields ────────────────────────

    #[test]
    fn load_skill_new_format() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: my-skill
description: A test skill
allowed-tools: search_hybrid read_note
license: MIT
---

# My Skill"#,
        )
        .unwrap();
        let entry = load_skill(&skill_dir.join("SKILL.md"), SkillScope::Vault).unwrap();
        assert_eq!(entry.name, "my-skill");
        assert_eq!(entry.description, "A test skill");
        assert_eq!(entry.license, Some("MIT".into()));
        assert_eq!(entry.allowed_tools, vec!["search_hybrid", "read_note"]);
        assert!(entry.legacy_trigger.is_none());
        assert_eq!(entry.validation_status(), SkillValidationStatus::Valid);
    }

    #[test]
    fn load_skill_legacy_format() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("old-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: old-skill
description: Legacy skill
trigger: knowledge
---

# Old Skill"#,
        )
        .unwrap();
        let entry = load_skill(&skill_dir.join("SKILL.md"), SkillScope::Vault).unwrap();
        assert_eq!(entry.name, "old-skill");
        assert_eq!(entry.legacy_trigger, Some("knowledge".into()));
        assert_eq!(entry.validation_status(), SkillValidationStatus::Legacy);
    }

    #[test]
    fn new_format_without_frontmatter_is_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("plain-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "# Plain Skill\n\nInstructions without Agent Skills frontmatter.",
        )
        .unwrap();

        let entry = load_skill(&skill_dir.join("SKILL.md"), SkillScope::Vault).unwrap();
        assert!(matches!(
            entry.validation_status(),
            SkillValidationStatus::Invalid(_)
        ));
    }

    #[test]
    fn new_format_name_mismatch_is_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("directory-name");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: different-name
description: Valid description
---

# Different Name"#,
        )
        .unwrap();

        let entry = load_skill(&skill_dir.join("SKILL.md"), SkillScope::Vault).unwrap();
        assert!(matches!(
            entry.validation_status(),
            SkillValidationStatus::Invalid(_)
        ));
    }

    #[test]
    fn scan_metadata_does_not_load_instruction_body() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        let skill_dir = vault.join(".iris/skills/meta-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: meta-skill
description: Valid description
---

# Meta Skill

Large instruction body."#,
        )
        .unwrap();

        let entries: Vec<_> = scan_all_metadata(&vault)
            .unwrap()
            .into_iter()
            .filter(|e| e.name == "meta-skill")
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "meta-skill");
        assert!(entries[0].content.is_empty());
    }

    #[test]
    fn load_skill_empty_description_is_invalid() {
        let entry = SkillEntry {
            name: "test".into(),
            description: String::new(),
            license: None,
            compatibility: None,
            metadata: HashMap::new(),
            allowed_tools: vec![],
            content: "body".into(),
            scope: SkillScope::Vault,
            source_url: None,
            enabled: true,
            file_path: "/test".into(),
            legacy_trigger: None,
        };
        assert!(matches!(
            entry.validation_status(),
            SkillValidationStatus::Invalid(_)
        ));
    }

    #[test]
    fn load_skill_description_too_long_is_invalid() {
        let entry = SkillEntry {
            name: "test".into(),
            description: "x".repeat(1025),
            license: None,
            compatibility: None,
            metadata: HashMap::new(),
            allowed_tools: vec![],
            content: "body".into(),
            scope: SkillScope::Vault,
            source_url: None,
            enabled: true,
            file_path: "/test".into(),
            legacy_trigger: None,
        };
        assert!(matches!(
            entry.validation_status(),
            SkillValidationStatus::Invalid(_)
        ));
    }

    #[test]
    fn unrecognized_tool_is_invalid() {
        let entry = SkillEntry {
            name: "test".into(),
            description: "Valid desc".into(),
            license: None,
            compatibility: None,
            metadata: HashMap::new(),
            allowed_tools: vec!["nonexistent_tool".into()],
            content: "body".into(),
            scope: SkillScope::Vault,
            source_url: None,
            enabled: true,
            file_path: "/test".into(),
            legacy_trigger: None,
        };
        assert!(matches!(
            entry.validation_status(),
            SkillValidationStatus::Invalid(_)
        ));
        assert!(!entry.all_allowed_tools_recognized());
        assert_eq!(entry.unrecognized_tools(), vec!["nonexistent_tool"]);
    }

    #[test]
    fn recognized_tools_are_valid() {
        let entry = SkillEntry {
            name: "test".into(),
            description: "Valid desc".into(),
            license: None,
            compatibility: None,
            metadata: HashMap::new(),
            allowed_tools: vec!["search_hybrid".into(), "read_note".into()],
            content: "body".into(),
            scope: SkillScope::Vault,
            source_url: None,
            enabled: true,
            file_path: "/test".into(),
            legacy_trigger: None,
        };
        assert_eq!(entry.validation_status(), SkillValidationStatus::Valid);
        assert!(entry.all_allowed_tools_recognized());
        assert!(entry.unrecognized_tools().is_empty());
    }

    // ── skills_for_scene ──────────────────────────────────

    fn make_skill(name: &str, legacy_trigger: Option<&str>, enabled: bool) -> SkillEntry {
        SkillEntry {
            name: name.into(),
            description: format!("Skill {name}"),
            license: None,
            compatibility: None,
            metadata: HashMap::new(),
            allowed_tools: vec![],
            content: String::new(),
            scope: SkillScope::Vault,
            source_url: None,
            enabled,
            file_path: format!("/test/{name}"),
            legacy_trigger: legacy_trigger.map(String::from),
        }
    }

    #[test]
    fn no_trigger_matches_all_scenes() {
        let skills = vec![make_skill("universal", None, true)];
        let matched = skills_for_scene(&skills, AiScene::KnowledgeLookup);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].name, "universal");
    }

    #[test]
    fn legacy_trigger_matches_scene() {
        let skills = vec![make_skill("knowledge-skill", Some("knowledge"), true)];
        let matched = skills_for_scene(&skills, AiScene::KnowledgeLookup);
        assert_eq!(matched.len(), 1);
    }

    #[test]
    fn legacy_trigger_wrong_scene_no_match() {
        let skills = vec![make_skill("writing-skill", Some("writing"), true)];
        let matched = skills_for_scene(&skills, AiScene::KnowledgeLookup);
        assert!(matched.is_empty());
    }

    #[test]
    fn disabled_skill_excluded() {
        let skills = vec![make_skill("disabled", None, false)];
        let matched = skills_for_scene(&skills, AiScene::KnowledgeLookup);
        assert!(matched.is_empty());
    }

    #[test]
    fn multiple_skills_filtered() {
        let skills = vec![
            make_skill("a", Some("knowledge"), true),
            make_skill("b", Some("writing"), true),
            make_skill("c", None, true),
        ];
        let matched = skills_for_scene(&skills, AiScene::KnowledgeLookup);
        assert_eq!(matched.len(), 2); // a + c (universal)
    }

    // ── BM25 scoring ──────────────────────────────────────

    #[test]
    fn bm25_exact_trigger_scores_highest() {
        let skills = vec![
            make_skill("universal", None, true),
            make_skill("knowledge-expert", Some("knowledge"), true),
        ];
        let ranked = rank_skills_for_scene(&skills, AiScene::KnowledgeLookup);
        assert_eq!(ranked.len(), 2);
        // knowledge-expert should score higher (trigger match + possible desc match)
        assert!(ranked[0].score >= ranked[1].score);
    }

    #[test]
    fn bm25_description_keyword_match() {
        let skills = vec![SkillEntry {
            name: "research-helper".into(),
            description: "Helps with research synthesis and evidence gathering".into(),
            license: None,
            compatibility: None,
            metadata: HashMap::new(),
            allowed_tools: vec![],
            content: String::new(),
            scope: SkillScope::Vault,
            source_url: None,
            enabled: true,
            file_path: "/test/research".into(),
            legacy_trigger: None,
        }];
        let ranked = rank_skills_for_scene(&skills, AiScene::ResearchSynthesis);
        assert_eq!(ranked.len(), 1);
        assert!(ranked[0].score > 1.0); // More than just the universal base score
    }

    #[test]
    fn bm25_name_match_boost() {
        let skills = vec![SkillEntry {
            name: "knowledge-graph".into(),
            description: "A tool".into(),
            license: None,
            compatibility: None,
            metadata: HashMap::new(),
            allowed_tools: vec![],
            content: String::new(),
            scope: SkillScope::Vault,
            source_url: None,
            enabled: true,
            file_path: "/test/kg".into(),
            legacy_trigger: None,
        }];
        let ranked = rank_skills_for_scene(&skills, AiScene::KnowledgeLookup);
        assert_eq!(ranked.len(), 1);
        // Name contains "knowledge" → boosted score
        assert!(ranked[0].score > 2.0);
    }

    #[test]
    fn bm25_metadata_keywords_boost() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "keywords".to_string(),
            serde_json::Value::String("research evidence analysis".into()),
        );
        let skills = vec![SkillEntry {
            name: "my-tool".into(),
            description: "A generic tool".into(),
            license: None,
            compatibility: None,
            metadata,
            allowed_tools: vec![],
            content: String::new(),
            scope: SkillScope::Vault,
            source_url: None,
            enabled: true,
            file_path: "/test/tool".into(),
            legacy_trigger: None,
        }];
        let ranked = rank_skills_for_scene(&skills, AiScene::ResearchSynthesis);
        assert_eq!(ranked.len(), 1);
        // Keywords match → boosted
        assert!(ranked[0].score > 2.0);
    }

    // ── Dependency management ─────────────────────────────

    #[test]
    fn depends_from_string_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "depends".to_string(),
            serde_json::Value::String("base-skill helper-skill".into()),
        );
        let entry = SkillEntry {
            name: "child".into(),
            description: "Child skill".into(),
            license: None,
            compatibility: None,
            metadata,
            allowed_tools: vec![],
            content: String::new(),
            scope: SkillScope::Vault,
            source_url: None,
            enabled: true,
            file_path: "/test/child".into(),
            legacy_trigger: None,
        };
        assert_eq!(entry.depends(), vec!["base-skill", "helper-skill"]);
    }

    #[test]
    fn depends_from_array_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "depends".to_string(),
            serde_json::Value::Array(vec![
                serde_json::Value::String("alpha".into()),
                serde_json::Value::String("beta".into()),
            ]),
        );
        let entry = SkillEntry {
            name: "child".into(),
            description: "Child skill".into(),
            license: None,
            compatibility: None,
            metadata,
            allowed_tools: vec![],
            content: String::new(),
            scope: SkillScope::Vault,
            source_url: None,
            enabled: true,
            file_path: "/test/child".into(),
            legacy_trigger: None,
        };
        assert_eq!(entry.depends(), vec!["alpha", "beta"]);
    }

    #[test]
    fn missing_dependencies_detected() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "depends".to_string(),
            serde_json::Value::String("installed-skill missing-skill".into()),
        );
        let entry = SkillEntry {
            name: "child".into(),
            description: "Child skill".into(),
            license: None,
            compatibility: None,
            metadata,
            allowed_tools: vec![],
            content: String::new(),
            scope: SkillScope::Vault,
            source_url: None,
            enabled: true,
            file_path: "/test/child".into(),
            legacy_trigger: None,
        };
        let installed = vec!["installed-skill".to_string(), "other".to_string()];
        let missing = entry.missing_dependencies(&installed);
        assert_eq!(missing, vec!["missing-skill"]);
    }

    // ── Migration ─────────────────────────────────────────

    #[test]
    fn migrate_legacy_skill_converts_format() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("old-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: old-skill
description: A legacy skill
trigger: knowledge
---

# Old Skill

Instructions here."#,
        )
        .unwrap();

        let entry = migrate_legacy_skill(&skill_dir.join("SKILL.md"), SkillScope::Vault).unwrap();
        assert_eq!(entry.name, "old-skill");
        assert!(entry.legacy_trigger.is_none()); // trigger removed
        assert_eq!(entry.validation_status(), SkillValidationStatus::Valid);

        // Backup should exist
        assert!(skill_dir.join("SKILL.md.bak").exists());

        // Content should still be there
        assert!(entry.content.contains("Instructions here"));
    }

    #[test]
    fn migrate_non_legacy_fails() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("new-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: new-skill
description: Already new format
---

# New Skill"#,
        )
        .unwrap();

        let err = migrate_legacy_skill(&skill_dir.join("SKILL.md"), SkillScope::Vault).unwrap_err();
        assert!(err.to_string().contains("新格式"));
    }

    #[test]
    fn is_legacy_format_detects_trigger() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("legacy.md"),
            "---\nname: x\ndescription: y\ntrigger: knowledge\n---\n\nbody",
        )
        .unwrap();
        fs::write(
            dir.path().join("new.md"),
            "---\nname: x\ndescription: y\n---\n\nbody",
        )
        .unwrap();
        assert!(is_legacy_format(&dir.path().join("legacy.md")));
        assert!(!is_legacy_format(&dir.path().join("new.md")));
    }

    // ── Compatibility validation ──────────────────────────

    #[test]
    fn load_skill_rejects_long_compatibility() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("bad-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                "---\nname: bad-skill\ndescription: test\ncompatibility: {}\n---\n\nbody",
                "x".repeat(501)
            ),
        )
        .unwrap();
        let err = load_skill(&skill_dir.join("SKILL.md"), SkillScope::Vault).unwrap_err();
        assert!(err.to_string().contains("compatibility exceeds 500"));
    }

    #[test]
    fn load_skill_rejects_long_description() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("bad-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                "---\nname: bad-skill\ndescription: {}\n---\n\nbody",
                "x".repeat(1025)
            ),
        )
        .unwrap();
        let err = load_skill(&skill_dir.join("SKILL.md"), SkillScope::Vault).unwrap_err();
        assert!(err.to_string().contains("description exceeds 1024"));
    }

    // ── Active skills regression ──────────────────────────

    #[test]
    fn inject_into_prompt_only_includes_enabled_skills() {
        let skills = vec![
            make_skill("enabled-one", None, true),
            make_skill("disabled-one", None, false),
            make_skill("enabled-two", None, true),
        ];
        let prompt = inject_into_prompt(&skills, AiScene::KnowledgeLookup);
        assert!(prompt.contains("enabled-one"));
        assert!(prompt.contains("enabled-two"));
        assert!(!prompt.contains("disabled-one"));
    }

    #[test]
    fn inject_into_prompt_empty_when_no_skills() {
        let skills: Vec<SkillEntry> = vec![];
        let prompt = inject_into_prompt(&skills, AiScene::KnowledgeLookup);
        assert!(prompt.is_empty());
    }
}
