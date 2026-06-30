use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillManifestKind {
    LegacyPromptOnly,
    PromptOnly,
    Resource,
    Workspace,
    McpDependent,
    Hybrid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrisSkillManifest {
    pub schema_version: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub kind: SkillManifestKind,
    #[serde(default)]
    pub compatibility: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub prompt: SkillPromptContract,
    #[serde(default)]
    pub resources: SkillResourceContract,
    #[serde(default)]
    pub workspace: SkillWorkspaceContract,
    #[serde(default)]
    pub capabilities: SkillCapabilityContract,
    #[serde(default)]
    pub mcp: SkillMcpContract,
    #[serde(default)]
    pub degradation: SkillDegradationPolicy,
    #[serde(default)]
    pub metadata: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillPromptContract {
    #[serde(default)]
    pub default_sections: Vec<String>,
    #[serde(default)]
    pub sections: Vec<PromptSection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSection {
    pub id: String,
    pub source: String,
    #[serde(default)]
    pub requires_runtime: bool,
    #[serde(default)]
    pub requires_capabilities: Vec<String>,
    #[serde(default)]
    pub requires_resources: Vec<String>,
    #[serde(default)]
    pub requires_workspace: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillResourceContract {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillWorkspaceContract {
    #[serde(default)]
    pub declared: bool,
    #[serde(default)]
    pub folders: Vec<String>,
    #[serde(default)]
    pub documents: Vec<SkillWorkspaceDocumentContract>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillWorkspaceDocumentContract {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillCapabilityContract {
    #[serde(default)]
    pub requires: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillMcpContract {
    #[serde(default)]
    pub dependencies: Vec<McpDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpDependency {
    pub profile_id: String,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillDegradationPolicy {
    #[serde(default = "default_runtime_missing_policy")]
    pub when_runtime_missing: String,
    #[serde(default)]
    pub message: Option<String>,
}

impl Default for SkillDegradationPolicy {
    fn default() -> Self {
        Self {
            when_runtime_missing: default_runtime_missing_policy(),
            message: None,
        }
    }
}

fn default_runtime_missing_policy() -> String {
    "not_applicable".to_string()
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManifestLoadOutcome {
    pub manifest: IrisSkillManifest,
    pub manifest_path: Option<PathBuf>,
    pub warnings: Vec<String>,
}

/// Load the Iris typed skill manifest for a skill directory.
pub fn load_manifest_for_skill_dir(
    skill_dir: &Path,
    frontmatter_manifest_path: Option<&str>,
) -> AppResult<ManifestLoadOutcome> {
    let manifest_path = match frontmatter_manifest_path {
        Some(relative) => Some(resolve_manifest_path(skill_dir, relative)?),
        None => {
            let default_path = skill_dir.join("iris.skill.toml");
            default_path.is_file().then_some(default_path)
        }
    };

    let Some(manifest_path) = manifest_path else {
        let name = skill_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("skill")
            .to_string();
        return Ok(ManifestLoadOutcome {
            manifest: IrisSkillManifest::legacy_prompt_only(name),
            manifest_path: None,
            warnings: vec!["legacy_prompt_only: no iris.skill.toml manifest found".into()],
        });
    };

    let raw = fs::read_to_string(&manifest_path)?;
    let warnings = validate_top_level_sections(&raw)?;
    let mut manifest: IrisSkillManifest = toml::from_str(&raw)
        .map_err(|err| AppError::msg(format!("invalid iris.skill.toml: {err}")))?;
    manifest.validate()?;

    Ok(ManifestLoadOutcome {
        manifest,
        manifest_path: Some(manifest_path),
        warnings,
    })
}

impl IrisSkillManifest {
    fn legacy_prompt_only(name: String) -> Self {
        Self {
            schema_version: "legacy".into(),
            name,
            version: None,
            kind: SkillManifestKind::LegacyPromptOnly,
            compatibility: None,
            license: None,
            prompt: SkillPromptContract::default(),
            resources: SkillResourceContract::default(),
            workspace: SkillWorkspaceContract::default(),
            capabilities: SkillCapabilityContract::default(),
            mcp: SkillMcpContract::default(),
            degradation: SkillDegradationPolicy::default(),
            metadata: BTreeMap::new(),
        }
    }

    fn validate(&mut self) -> AppResult<()> {
        if self.schema_version.trim().is_empty() {
            return Err(AppError::msg("manifest schema_version is required"));
        }
        if self.name.trim().is_empty() {
            return Err(AppError::msg("manifest name is required"));
        }
        normalize_string_list(&mut self.capabilities.requires);
        normalize_string_list(&mut self.resources.required);
        normalize_string_list(&mut self.resources.optional);
        for section in &mut self.prompt.sections {
            if section.id.trim().is_empty() {
                return Err(AppError::msg("prompt section id is required"));
            }
            if section.source.trim().is_empty() {
                return Err(AppError::msg("prompt section source is required"));
            }
            normalize_string_list(&mut section.requires_capabilities);
            normalize_string_list(&mut section.requires_resources);
        }
        for dep in &mut self.mcp.dependencies {
            if dep.profile_id.trim().is_empty() {
                return Err(AppError::msg("mcp dependency profile_id is required"));
            }
            normalize_string_list(&mut dep.required_capabilities);
        }
        Ok(())
    }
}

fn normalize_string_list(items: &mut Vec<String>) {
    for item in items.iter_mut() {
        *item = item.trim().to_string();
    }
    items.retain(|item| !item.is_empty());
    items.sort();
    items.dedup();
}

fn resolve_manifest_path(skill_dir: &Path, relative: &str) -> AppResult<PathBuf> {
    let candidate = Path::new(relative);
    if candidate.is_absolute()
        || candidate.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(AppError::msg("manifest path escapes skill directory"));
    }
    let resolved = skill_dir.join(candidate);
    if !resolved.is_file() {
        return Err(AppError::msg("manifest file not found"));
    }
    Ok(resolved)
}

fn validate_top_level_sections(raw: &str) -> AppResult<Vec<String>> {
    let value: toml::Value = toml::from_str(raw)
        .map_err(|err| AppError::msg(format!("invalid iris.skill.toml: {err}")))?;
    let Some(table) = value.as_table() else {
        return Err(AppError::msg("manifest must be a TOML table"));
    };
    let allowed: BTreeSet<&'static str> = [
        "schema_version",
        "name",
        "version",
        "kind",
        "compatibility",
        "license",
        "prompt",
        "resources",
        "workspace",
        "capabilities",
        "mcp",
        "degradation",
        "metadata",
    ]
    .into_iter()
    .collect();
    let security_sensitive: BTreeSet<&'static str> = [
        "runtime",
        "permissions",
        "permission",
        "process",
        "script",
        "scripts",
        "command",
        "commands",
        "dependencies",
        "secrets",
        "credentials",
    ]
    .into_iter()
    .collect();
    let mut warnings = Vec::new();
    for key in table.keys() {
        if allowed.contains(key.as_str()) {
            continue;
        }
        if security_sensitive.contains(key.as_str()) {
            return Err(AppError::msg(format!(
                "unsupported manifest section `{key}`"
            )));
        }
        warnings.push(format!("unknown manifest metadata section `{key}` ignored"));
    }
    validate_security_sensitive_nested_fields(table)?;
    reject_raw_secret_markers(&value, "manifest")?;
    Ok(warnings)
}

fn validate_security_sensitive_nested_fields(
    table: &toml::map::Map<String, toml::Value>,
) -> AppResult<()> {
    let nested_sensitive: BTreeSet<&'static str> = [
        "api_key",
        "bearer",
        "command",
        "commands",
        "credential",
        "credentials",
        "env",
        "password",
        "process",
        "script",
        "scripts",
        "secret",
        "secrets",
        "token",
    ]
    .into_iter()
    .collect();
    for root in [
        "mcp",
        "workspace",
        "capabilities",
        "permissions",
        "permission",
        "runtime",
    ] {
        if let Some(value) = table.get(root) {
            reject_sensitive_keys(value, root, &nested_sensitive)?;
        }
    }
    Ok(())
}

fn reject_sensitive_keys(
    value: &toml::Value,
    path: &str,
    sensitive: &BTreeSet<&'static str>,
) -> AppResult<()> {
    match value {
        toml::Value::Table(table) => {
            for (key, child) in table {
                let child_path = format!("{path}.{key}");
                if sensitive.contains(key.as_str()) {
                    return Err(AppError::msg(format!(
                        "unknown security-sensitive field `{child_path}`"
                    )));
                }
                reject_sensitive_keys(child, &child_path, sensitive)?;
            }
        }
        toml::Value::Array(items) => {
            for item in items {
                reject_sensitive_keys(item, path, sensitive)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn reject_raw_secret_markers(value: &toml::Value, path: &str) -> AppResult<()> {
    match value {
        toml::Value::String(raw) => {
            let lower = raw.to_ascii_lowercase();
            let looks_secret = lower.contains("sk-")
                || lower.contains("bearer ")
                || lower.contains("token=")
                || lower.contains("api_key=")
                || lower.contains("password=")
                || lower.contains("secret=");
            if looks_secret {
                return Err(AppError::msg(format!(
                    "raw secret marker is not allowed in `{path}`"
                )));
            }
        }
        toml::Value::Table(table) => {
            for (key, child) in table {
                reject_raw_secret_markers(child, &format!("{path}.{key}"))?;
            }
        }
        toml::Value::Array(items) => {
            for item in items {
                reject_raw_secret_markers(item, path)?;
            }
        }
        _ => {}
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn missing_manifest_is_prompt_only_without_runtime_requirement() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("simple-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: simple-skill\ndescription: Simple prompt skill\n---\n\nBody",
        )
        .unwrap();

        let outcome = load_manifest_for_skill_dir(&skill_dir, None).unwrap();

        assert_eq!(outcome.manifest.kind, SkillManifestKind::LegacyPromptOnly);
        assert!(!outcome.manifest.workspace.declared);
        assert!(outcome.manifest.capabilities.requires.is_empty());
        assert!(outcome.manifest.mcp.dependencies.is_empty());
        assert!(outcome
            .warnings
            .iter()
            .any(|w| w.contains("legacy_prompt_only")));
    }

    #[test]
    fn parses_prompt_only_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("legal-review");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("iris.skill.toml"),
            r#"
schema_version = "1"
name = "legal-review"
version = "1.0.0"
kind = "prompt_only"

[prompt]
default_sections = ["behavior"]

[[prompt.sections]]
id = "behavior"
source = "SKILL.md"
requires_runtime = false

[workspace]
declared = false

[capabilities]
requires = []

[degradation]
when_runtime_missing = "not_applicable"
"#,
        )
        .unwrap();

        let outcome = load_manifest_for_skill_dir(&skill_dir, None).unwrap();

        assert_eq!(outcome.manifest.name, "legal-review");
        assert_eq!(outcome.manifest.kind, SkillManifestKind::PromptOnly);
        assert_eq!(outcome.manifest.prompt.sections[0].id, "behavior");
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn parses_mcp_dependent_manifest_without_requiring_profile_to_exist() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("anysearch");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("iris.skill.toml"),
            r#"
schema_version = "1"
name = "anysearch"
kind = "mcp_dependent"

[prompt]
default_sections = ["behavior"]

[[prompt.sections]]
id = "behavior"
source = "SKILL.md"
requires_runtime = false

[workspace]
declared = false

[capabilities]
requires = ["web.search", "web.fetch"]

[[mcp.dependencies]]
profile_id = "anysearch"
required_capabilities = ["web.search", "web.fetch"]
required = true

[degradation]
when_runtime_missing = "partial"
message = "Enable AnySearch MCP profile to execute web search."
"#,
        )
        .unwrap();

        let outcome = load_manifest_for_skill_dir(&skill_dir, None).unwrap();

        assert_eq!(outcome.manifest.kind, SkillManifestKind::McpDependent);
        assert_eq!(
            outcome.manifest.capabilities.requires,
            vec!["web.fetch".to_string(), "web.search".to_string()]
        );
        assert_eq!(outcome.manifest.mcp.dependencies[0].profile_id, "anysearch");
    }

    #[test]
    fn frontmatter_manifest_path_takes_precedence_over_default_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("chooser");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("iris.skill.toml"),
            "schema_version = \"1\"\nname = \"default\"\nkind = \"prompt_only\"\n",
        )
        .unwrap();
        fs::write(
            skill_dir.join("custom.toml"),
            "schema_version = \"1\"\nname = \"custom\"\nkind = \"resource\"\n",
        )
        .unwrap();

        let outcome = load_manifest_for_skill_dir(&skill_dir, Some("custom.toml")).unwrap();

        assert_eq!(outcome.manifest.name, "custom");
        assert_eq!(outcome.manifest.kind, SkillManifestKind::Resource);
    }

    #[test]
    fn rejects_unknown_security_sensitive_field() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("bad");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("iris.skill.toml"),
            r#"
schema_version = "1"
name = "bad"
kind = "prompt_only"

[runtime]
command = "start.sh"
"#,
        )
        .unwrap();

        let err = load_manifest_for_skill_dir(&skill_dir, None).unwrap_err();

        assert!(err
            .to_string()
            .contains("unsupported manifest section `runtime`"));
    }

    #[test]
    fn rejects_embedded_mcp_command_in_dependency() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("bad-mcp-command");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("iris.skill.toml"),
            r#"
schema_version = "1"
name = "bad-mcp-command"
kind = "mcp_dependent"

[[mcp.dependencies]]
profile_id = "bad"
command = "npx"
required = true
"#,
        )
        .unwrap();

        let err = load_manifest_for_skill_dir(&skill_dir, None).unwrap_err();

        assert!(err
            .to_string()
            .contains("unknown security-sensitive field `mcp.dependencies.command`"));
    }

    #[test]
    fn unknown_metadata_section_warns_without_blocking_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("metadata-warning");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("iris.skill.toml"),
            r#"
schema_version = "1"
name = "metadata-warning"
kind = "prompt_only"

[x-skillhub]
rating = "experimental"
"#,
        )
        .unwrap();

        let outcome = load_manifest_for_skill_dir(&skill_dir, None).unwrap();

        assert_eq!(outcome.manifest.kind, SkillManifestKind::PromptOnly);
        assert!(outcome
            .warnings
            .iter()
            .any(|warning| warning
                .contains("unknown manifest metadata section `x-skillhub` ignored")));
    }

    #[test]
    fn rejects_raw_secret_markers_anywhere_in_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("raw-secret");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("iris.skill.toml"),
            r#"
schema_version = "1"
name = "raw-secret"
kind = "prompt_only"

[metadata]
example = "sk-live-secret"
"#,
        )
        .unwrap();

        let err = load_manifest_for_skill_dir(&skill_dir, None).unwrap_err();

        assert!(err.to_string().contains("raw secret marker"));
    }
    #[test]
    fn rejects_manifest_paths_that_escape_skill_dir() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("escape");
        fs::create_dir_all(&skill_dir).unwrap();

        let err = load_manifest_for_skill_dir(&skill_dir, Some("../iris.skill.toml")).unwrap_err();

        assert!(err
            .to_string()
            .contains("manifest path escapes skill directory"));
    }
}
