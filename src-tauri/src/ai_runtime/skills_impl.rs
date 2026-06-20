//! Agent Skills runtime 鈥?SKILL.md registry, validation, matching, prompt injection.
//!
//! Compatible with Agent Skills specification while preserving Iris local-first
//! security model. Old `trigger`-based skills continue to work via `legacy_trigger`.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use hex;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

#[rustfmt::skip]
const SAFE_GIT_CLONE_ARGS: &[&str] = &["-c", "core.hooksPath=/dev/null", "-c", "filter.lfs.smudge=", "-c", "filter.lfs.required=false", "-c", "protocol.file.allow=never", "clone", "--depth", "1", "--no-tags", "--"];

fn run_git_clone_with_timeout(repo_url: &str, target_dir: &Path) -> AppResult<()> {
    let mut child = std::process::Command::new("git")
        .env_clear()
        .env("LANG", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .args(SAFE_GIT_CLONE_ARGS)
        .arg(repo_url)
        .arg(target_dir.to_str().unwrap_or(""))
        .spawn()
        .map_err(|e| AppError::msg(format!("git not available: {e}")))?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            if status.success() {
                return Ok(());
            }
            return Err(AppError::msg("git clone failed"));
        }
        if start.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AppError::msg("git clone timed out"));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[path = "skills/activation.rs"]
mod activation_impl;
#[path = "skills/compatibility.rs"]
mod compatibility_impl;
#[path = "skills/frontmatter.rs"]
mod frontmatter_impl;
#[path = "skills/legacy.rs"]
mod legacy_impl;
#[path = "skills/model.rs"]
mod model_impl;
#[path = "skills/path.rs"]
mod path_impl;
#[path = "skills/prompt.rs"]
mod prompt_impl;
#[path = "skills/resources.rs"]
mod resources_impl;
#[path = "skills/scan.rs"]
mod scan_impl;
#[path = "skills/validation.rs"]
mod validation_impl;
#[path = "skills/workspace.rs"]
mod workspace_impl;

pub use activation_impl::{
    active_skill_allowed_tools, active_skill_allowed_tools_for_task, active_skills_for_prompt,
    active_skills_for_task_prompt, build_skill_activation_plan,
    build_skill_activation_plan_for_task, enrich_list_with_scene, enrich_list_with_task,
    load_activation_index, rank_skills_for_scene, rank_skills_for_scene_with_index,
    rank_skills_for_task, rerank_skills_with_vectors, skills_for_scene, skills_for_task,
};
pub use compatibility_impl::{
    blocked_capabilities_for_skill, fallback_guidance, normalize_external_capability,
    support_status_for_capability,
};
use frontmatter_impl::parse_frontmatter;
pub use legacy_impl::{is_legacy_format, migrate_legacy_skill};
pub use model_impl::{
    ActivationIndexMap, ScoredSkill, SkillActivationIndexRow, SkillEntry, SkillListEntry,
    SkillMetadata, SkillScope, SkillValidationStatus, SkillWorkspaceDocument,
    SkillWorkspaceManifest,
};
pub use path_impl::validate_skill_path;
use path_impl::{atomic_copy_dir, load_config, save_config, skill_key, slugify, validate_subpath};
pub(crate) use path_impl::{global_skills_dir, vault_skills_dir};
pub use prompt_impl::inject_into_prompt;
pub use resources_impl::read_skill_resource;
pub use scan_impl::{
    load_skill, scan_all, scan_all_metadata, scan_all_with_status, skill_content_hash_for_path,
};
pub use validation_impl::{
    capability_preview_for_entry, confirmation_required_tools, license_is_agpl_compatible,
    validate_skill_license,
};
pub use workspace_impl::{
    list_workspace_files, prepare_workspace_for_skill, preview_prepare_workspace,
    read_workspace_file, validate_workspace_folder_path, validate_workspace_source_path,
    validate_workspace_target_path, workspace_manifest_items, workspace_root_path,
    workspace_root_relative, workspace_status_for_skill, write_workspace_file,
    SkillWorkspacePrepareResult, SkillWorkspaceStatus,
};

/// Install skill from HTTP(S) URL (raw SKILL.md or GitHub raw link).
pub async fn install_from_url(
    url: &str,
    scope: SkillScope,
    vault: &Path,
    expected_sha256: Option<&str>,
) -> AppResult<SkillEntry> {
    crate::security::ipc_policy::validate_skill_remote_url(url)?;
    let client = crate::network::cert_pinning::create_https_client()?;
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

    if let Some(expected) = expected_sha256 {
        let actual = hex::encode(Sha256::digest(body.as_bytes()));
        if !actual.eq_ignore_ascii_case(expected.trim()) {
            return Err(AppError::msg(
                "Skill 内容 SHA-256 校验失败（可能被篡改或不完整）",
            ));
        }
        tracing::info!(
            url = %url,
            sha256 = %actual,
            "skill content hash verified"
        );
    }

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

    let tmp = crate::security::secure_delete::user_temp_dir()
        .join(format!("iris-skill-{}", uuid::Uuid::new_v4()));
    if let Some(parent) = tmp.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::msg(format!("无法创建临时目录: {e}")))?;
    }
    run_git_clone_with_timeout(repo_url, &tmp)?;

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

    let _ = crate::security::secure_delete::secure_remove_dir_all(&tmp);
    if installed.is_empty() {
        return Err(AppError::msg("no SKILL.md found in repository"));
    }
    Ok(installed)
}

pub fn uninstall(name: &str, scope: SkillScope, vault: &Path) -> AppResult<()> {
    let base = match scope {
        SkillScope::Global => global_skills_dir(),
        SkillScope::Vault => vault_skills_dir(vault),
    };
    if base.is_dir() {
        for entry in fs::read_dir(&base)? {
            let entry = entry?;
            let path = entry.path();
            let skill_file = path.join("SKILL.md");
            if skill_file.is_file() {
                if let Ok(skill) = load_skill(&skill_file, scope) {
                    if skill.name == name {
                        fs::remove_dir_all(path)?;
                        return Ok(());
                    }
                }
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::ai_runtime::AiScene;

    use super::*;

    // 鈹€鈹€ validate_subpath 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

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

    // 鈹€鈹€ atomic_copy_dir 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

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

    // 鈹€鈹€ slugify 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("My Skill"), "my-skill");
        assert_eq!(slugify("hello_world"), "hello_world");
        assert_eq!(slugify("a/b\\c"), "a-b-c");
    }

    #[test]
    fn yaml_frontmatter_supports_arrays_and_objects() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("yaml-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        let path = skill_dir.join("SKILL.md");
        fs::write(
            &path,
            r#"---
name: yaml-skill
description: Parses modern Agent Skills frontmatter
allowed-tools:
  - memory_read
  - skills_read_resource
metadata:
  depends:
    - helper-skill
  keywords:
    - research
    - memory
license: AGPL-3.0
---

# Body
"#,
        )
        .unwrap();

        let skill = load_skill(&path, SkillScope::Global).unwrap();
        assert_eq!(
            skill.allowed_tools,
            vec![
                "memory_read".to_string(),
                "skills_read_resource".to_string()
            ]
        );
        assert_eq!(skill.depends(), vec!["helper-skill".to_string()]);
        assert_eq!(skill.license.as_deref(), Some("AGPL-3.0"));
    }

    #[test]
    fn load_skill_reads_workspace_manifest_from_top_level_and_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let top_level_dir = dir.path().join("top-level-workspace");
        fs::create_dir_all(&top_level_dir).unwrap();
        let top_level_path = top_level_dir.join("SKILL.md");
        fs::write(
            &top_level_path,
            r#"---
name: top-level-workspace
description: Reads top-level workspace declaration
iris-workspace:
  folders:
    - inputs
    - outputs
  documents:
    - source: resources/default-note.md
      target: README.md
---

# Body
"#,
        )
        .unwrap();

        let metadata_dir = dir.path().join("metadata-workspace");
        fs::create_dir_all(&metadata_dir).unwrap();
        let metadata_path = metadata_dir.join("SKILL.md");
        fs::write(
            &metadata_path,
            r#"---
name: metadata-workspace
description: Reads metadata workspace declaration
metadata:
  iris_workspace:
    folders:
      - cache
    documents:
      - source: references/guide.md
        target: docs/guide.md
---

# Body
"#,
        )
        .unwrap();

        let top_level = load_skill(&top_level_path, SkillScope::Vault).unwrap();
        let metadata = load_skill(&metadata_path, SkillScope::Vault).unwrap();

        let top_level_workspace = top_level.workspace_manifest().unwrap();
        assert_eq!(top_level_workspace.folders, vec!["inputs", "outputs"]);
        assert_eq!(top_level_workspace.documents[0].target, "README.md");

        let metadata_workspace = metadata.workspace_manifest().unwrap();
        assert_eq!(metadata_workspace.folders, vec!["cache"]);
        assert_eq!(
            metadata_workspace.documents[0].source,
            "references/guide.md"
        );
        assert_eq!(metadata_workspace.documents[0].target, "docs/guide.md");
    }

    #[test]
    fn uninstall_removes_actual_skill_dir_when_name_mismatches_dir() {
        let vault_dir = tempfile::tempdir().unwrap();
        let vault = vault_dir.path();
        let skill_root = vault.join(".iris").join("skills").join("custom-dir");
        fs::create_dir_all(&skill_root).unwrap();
        fs::write(
            skill_root.join("SKILL.md"),
            r#"---
name: displayed-name
description: Directory and name intentionally differ
---

Body
"#,
        )
        .unwrap();

        uninstall("displayed-name", SkillScope::Vault, vault).unwrap();
        assert!(
            !skill_root.exists(),
            "uninstall should remove the directory containing the matching SKILL.md"
        );
    }

    #[test]
    fn capability_preview_reports_requested_and_missing_tools() {
        let entry = SkillEntry {
            name: "preview".into(),
            description: "Preview".into(),
            license: Some("AGPL-3.0".into()),
            compatibility: None,
            metadata: Default::default(),
            allowed_tools: vec![
                "memory_write".into(),
                "fetch_web_page".into(),
                "totally_unknown".into(),
            ],
            content: String::new(),
            scope: SkillScope::Global,
            source_url: None,
            enabled: true,
            file_path: String::new(),
            legacy_trigger: None,
        };

        let preview = capability_preview_for_entry(&entry, &[]);
        assert_eq!(preview["license"], "AGPL-3.0");
        assert!(preview["requested_tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "memory_write"));
        assert!(preview["confirmation_required_tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "fetch_web_page"));
        assert!(preview["unrecognized_tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "totally_unknown"));
    }

    // 鈹€鈹€ install_from_git symlink escape check 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

    #[allow(unused_variables)]
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

    // 鈹€鈹€ frontmatter parsing 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

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

    // 鈹€鈹€ load_skill with new fields 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

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
    fn unrecognized_tool_is_partial_diagnostic_not_invalid() {
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
        assert_eq!(entry.validation_status(), SkillValidationStatus::Valid);
        assert!(!entry.all_allowed_tools_recognized());
        assert_eq!(entry.unrecognized_tools(), vec!["nonexistent_tool"]);
        assert_eq!(entry.blocked_capabilities().len(), 1);
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

    // 鈹€鈹€ skills_for_scene 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

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
        let matched = skills_for_scene(&skills, AiScene::KnowledgeLookup, "");
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].name, "universal");
    }

    #[test]
    fn legacy_trigger_matches_scene() {
        let skills = vec![make_skill("knowledge-skill", Some("knowledge"), true)];
        let matched = skills_for_scene(&skills, AiScene::KnowledgeLookup, "");
        assert_eq!(matched.len(), 1);
    }

    #[test]
    fn legacy_trigger_wrong_scene_no_match() {
        let skills = vec![make_skill("writing-skill", Some("writing"), true)];
        let matched = skills_for_scene(&skills, AiScene::KnowledgeLookup, "");
        assert!(matched.is_empty());
    }

    #[test]
    fn disabled_skill_excluded() {
        let skills = vec![make_skill("disabled", None, false)];
        let matched = skills_for_scene(&skills, AiScene::KnowledgeLookup, "");
        assert!(matched.is_empty());
    }

    #[test]
    fn multiple_skills_filtered() {
        let skills = vec![
            make_skill("a", Some("knowledge"), true),
            make_skill("b", Some("writing"), true),
            make_skill("c", None, true),
        ];
        let matched = skills_for_scene(&skills, AiScene::KnowledgeLookup, "");
        assert_eq!(matched.len(), 2); // a + c (universal)
    }

    // 鈹€鈹€ BM25 scoring 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

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
        // Name contains "knowledge" 鈫?boosted score
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
        // Keywords match 鈫?boosted
        assert!(ranked[0].score > 2.0);
    }

    // 鈹€鈹€ Dependency management 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

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

    // 鈹€鈹€ Migration 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

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

    // 鈹€鈹€ Compatibility validation 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

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

    // 鈹€鈹€ Active skills regression 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

    #[test]
    fn inject_into_prompt_only_includes_enabled_skills() {
        let vault = tempfile::tempdir().unwrap();
        let skills = vec![
            make_skill("enabled-one", None, true),
            make_skill("disabled-one", None, false),
            make_skill("enabled-two", None, true),
        ];
        let prompt = inject_into_prompt(vault.path(), &skills, AiScene::KnowledgeLookup, "");
        assert!(prompt.contains("enabled-one"));
        assert!(prompt.contains("enabled-two"));
        assert!(!prompt.contains("disabled-one"));
    }

    #[test]
    fn inject_into_prompt_empty_when_no_skills() {
        let vault = tempfile::tempdir().unwrap();
        let skills: Vec<SkillEntry> = vec![];
        let prompt = inject_into_prompt(vault.path(), &skills, AiScene::KnowledgeLookup, "");
        assert!(prompt.is_empty());
    }

    #[test]
    fn inject_into_prompt_truncates_large_skill_body() {
        let vault = tempfile::tempdir().unwrap();
        let mut skill = make_skill("large-skill", None, true);
        skill.content = format!("start\n{}\nend", "x".repeat(80_000));

        let prompt = inject_into_prompt(vault.path(), &[skill], AiScene::KnowledgeLookup, "");

        assert!(prompt.contains("large-skill"));
        assert!(prompt.contains("start"));
        assert!(prompt.contains("[skill content truncated"));
        assert!(!prompt.contains("\nend\n"));
        assert!(prompt.len() < 30_000);
    }

    #[test]
    fn read_skill_resource_allows_declared_resource_dirs_only() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        let skill_root = vault.join(".iris/skills/my-skill");
        for dir in ["references", "resources", "scripts"] {
            std::fs::create_dir_all(skill_root.join(dir)).unwrap();
        }
        std::fs::write(skill_root.join("references/guide.md"), "guide body").unwrap();
        std::fs::write(skill_root.join("resources/data.md"), "resource body").unwrap();
        std::fs::write(skill_root.join("scripts/tool.sh"), "script body").unwrap();
        std::fs::write(skill_root.join("SKILL.md"), "# Skill").unwrap();
        let read = |path| read_skill_resource(&vault, "my-skill", SkillScope::Vault, path);

        assert_eq!(read("references/guide.md").unwrap(), "guide body");
        assert_eq!(read("resources/data.md").unwrap(), "resource body");
        assert!(read("../SKILL.md").is_err());
        assert!(read("scripts/tool.sh").is_err());
        assert!(read("notes/secret.md").is_err());
    }
}
