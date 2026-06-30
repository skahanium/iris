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
        for section in &mut self.prompt.sections {
            if section.id.trim().is_empty() {
                return Err(AppError::msg("prompt section id is required"));
            }
            if section.source.trim().is_empty() {
                return Err(AppError::msg("prompt section source is required"));
            }
        }
        Ok(())
    }
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
    for root in ["metadata", "permissions", "permission", "runtime"] {
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

    #[cfg(test)]
    #[derive(Debug, PartialEq, Eq)]
    struct ParsedSkillMarkdownForTest {
        name: String,
        scope_rules: Vec<ParsedSkillScopeRuleForTest>,
    }

    #[cfg(test)]
    #[derive(Debug, PartialEq, Eq)]
    struct ParsedSkillScopeRuleForTest {
        kind: String,
        pattern: String,
    }

    #[cfg(test)]
    fn load_manifest_from_str_for_test(raw: &str) -> AppResult<IrisSkillManifest> {
        let mut manifest: IrisSkillManifest = toml::from_str(raw)
            .map_err(|err| AppError::msg(format!("invalid iris.skill.toml: {err}")))?;
        manifest.validate()?;
        Ok(manifest)
    }

    #[cfg(test)]
    fn parse_skill_markdown_for_test(raw: &str) -> AppResult<ParsedSkillMarkdownForTest> {
        #[derive(Deserialize)]
        struct Frontmatter {
            name: String,
            #[serde(default)]
            scope: Vec<ScopeRule>,
        }

        #[derive(Deserialize)]
        struct ScopeRule {
            kind: String,
            pattern: String,
        }

        let trimmed = raw.trim_start();
        let Some(rest) = trimmed.strip_prefix("---") else {
            return Err(AppError::msg("missing SKILL.md frontmatter"));
        };
        let Some(end) = rest.find("\n---") else {
            return Err(AppError::msg("unterminated SKILL.md frontmatter"));
        };
        let frontmatter: Frontmatter = serde_yaml::from_str(&rest[..end])
            .map_err(|err| AppError::msg(format!("invalid SKILL.md frontmatter: {err}")))?;
        Ok(ParsedSkillMarkdownForTest {
            name: frontmatter.name,
            scope_rules: frontmatter
                .scope
                .into_iter()
                .map(|rule| ParsedSkillScopeRuleForTest {
                    kind: rule.kind,
                    pattern: rule.pattern,
                })
                .collect(),
        })
    }

    #[test]
    fn rejects_unknown_manifest_kind() {
        let raw = r#"
schema_version = "1"
name = "bad"
kind = "runtime_bound"
"#;
        let err = load_manifest_from_str_for_test(raw)
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown variant"), "{err}");
    }

    #[test]
    fn parses_prompt_only_scope_from_skill_md_frontmatter() {
        let raw = r#"---
name: daily-review
description: Review daily notes
scope:
  - kind: glob
    pattern: "Daily/*.md"
---

Use the user's daily notes.
"#;
        let parsed = parse_skill_markdown_for_test(raw).unwrap();
        assert_eq!(parsed.name, "daily-review");
        assert_eq!(parsed.scope_rules.len(), 1);
        assert_eq!(parsed.scope_rules[0].kind, "glob");
        assert_eq!(parsed.scope_rules[0].pattern, "Daily/*.md");
    }

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
    fn rejects_runtime_bound_manifest_sections() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("runtime-bound");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("iris.skill.toml"),
            r#"
schema_version = "1"
name = "runtime-bound"
kind = "prompt_only"

[prompt]
default_sections = ["behavior"]

[[prompt.sections]]
id = "behavior"
source = "SKILL.md"

[runtime]
profile = "external"
"#,
        )
        .unwrap();

        let err = load_manifest_for_skill_dir(&skill_dir, None).unwrap_err();

        assert!(err
            .to_string()
            .contains("unsupported manifest section `runtime`"));
    }

    #[test]
    fn frontmatter_manifest_path_rejects_unsupported_sections() {
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
            "schema_version = \"1\"\nname = \"custom\"\nkind = \"prompt_only\"\n[process]\ncommand = \"run\"\n",
        )
        .unwrap();

        let err = load_manifest_for_skill_dir(&skill_dir, Some("custom.toml")).unwrap_err();

        assert!(err
            .to_string()
            .contains("unsupported manifest section `process`"));
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
    fn rejects_embedded_command_in_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("bad-command");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("iris.skill.toml"),
            r#"
schema_version = "1"
name = "bad-command"
kind = "prompt_only"

[metadata]
command = "npx"
"#,
        )
        .unwrap();

        let err = load_manifest_for_skill_dir(&skill_dir, None).unwrap_err();

        assert!(err
            .to_string()
            .contains("unknown security-sensitive field `metadata.command`"));
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
