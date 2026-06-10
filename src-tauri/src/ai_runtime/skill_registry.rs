//! Skill registry adapters — resolve registry references to install specs.
//!
//! SkillHub (`skillhub.cn`) is the first supported registry. Resolution uses the
//! public API at `api.skillhub.tencent.com` (same backend as the SkillHub web UI).

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::error::{AppError, AppResult};

/// Supported registry identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegistryId {
    SkillHub,
}

impl RegistryId {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "skillhub" | "skillhub.cn" => Some(Self::SkillHub),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::SkillHub => "skillhub",
        }
    }
}

/// Native install source after registry resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillInstallSource {
    Url,
    Git,
    Local,
    Registry,
}

impl SkillInstallSource {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "url" => Some(Self::Url),
            "git" => Some(Self::Git),
            "local" => Some(Self::Local),
            "registry" => Some(Self::Registry),
            _ => None,
        }
    }

    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Url | Self::Registry => "url",
            Self::Git => "git",
            Self::Local => "local",
        }
    }
}

/// Resolved install specification.
#[derive(Debug, Clone)]
pub struct InstallSpec {
    pub source: SkillInstallSource,
    pub path_or_url: String,
    pub subpath: Option<String>,
    pub display_name: Option<String>,
}

/// Pluggable registry adapter (SkillHub is the first implementation).
pub trait SkillRegistryAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn resolve<'a>(
        &'a self,
        reference: &'a str,
    ) -> Pin<Box<dyn Future<Output = AppResult<InstallSpec>> + Send + 'a>>;
}

const SKILLHUB_API: &str = "https://api.skillhub.tencent.com";
const SKILLHUB_PAGE_HOST: &str = "skillhub.cn";

#[derive(Debug, Deserialize)]
struct SkillHubDetailResponse {
    skill: SkillHubSkillMeta,
    #[serde(default)]
    repo_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SkillHubSkillMeta {
    slug: String,
    #[serde(default)]
    display_name: Option<String>,
}

struct SkillHubRegistryAdapter;

impl SkillRegistryAdapter for SkillHubRegistryAdapter {
    fn id(&self) -> &'static str {
        "skillhub"
    }

    fn resolve<'a>(
        &'a self,
        reference: &'a str,
    ) -> Pin<Box<dyn Future<Output = AppResult<InstallSpec>> + Send + 'a>> {
        Box::pin(async move { resolve_skillhub(reference).await })
    }
}

static SKILLHUB_ADAPTER: SkillHubRegistryAdapter = SkillHubRegistryAdapter;

static REGISTRY_ADAPTERS: LazyLock<HashMap<&'static str, &'static dyn SkillRegistryAdapter>> =
    LazyLock::new(|| {
        let mut map: HashMap<&'static str, &'static dyn SkillRegistryAdapter> = HashMap::new();
        let hub: &'static dyn SkillRegistryAdapter = &SKILLHUB_ADAPTER;
        map.insert("skillhub", hub);
        map.insert("skillhub.cn", hub);
        map
    });

fn registry_adapter(registry: &str) -> AppResult<&'static dyn SkillRegistryAdapter> {
    REGISTRY_ADAPTERS
        .get(registry.trim().to_lowercase().as_str())
        .copied()
        .ok_or_else(|| AppError::msg(format!("未知注册表: {registry}")))
}

/// Normalize user reference → skill slug.
pub fn normalize_skillhub_reference(reference: &str) -> AppResult<String> {
    let trimmed = reference.trim();
    if trimmed.is_empty() {
        return Err(AppError::msg("skill 名称不能为空"));
    }

    if trimmed.contains("/install/") {
        return Err(AppError::msg(
            "这是 SkillHub 安装指南页，不是 skill 本体。请使用 skill 名称，例如 skills_install(source=registry, registry=skillhub, path_or_url=<skill名>)",
        ));
    }

    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        crate::security::ipc_policy::validate_https_url(trimmed)?;
        let lower = trimmed.to_lowercase();
        if !lower.contains(SKILLHUB_PAGE_HOST) {
            return Err(AppError::msg(format!(
                "SkillHub 注册表仅支持 {SKILLHUB_PAGE_HOST} 域名"
            )));
        }
        if let Some(idx) = lower.find("/skills/") {
            let rest = &trimmed[idx + "/skills/".len()..];
            let slug = rest.split(['/', '?', '#']).next().unwrap_or("").trim();
            if !slug.is_empty() && slug != "install" {
                return Ok(slug.to_string());
            }
        }
        return Err(AppError::msg(
            "无法从 SkillHub 页面 URL 提取 skill 名称，请直接使用 skill 名",
        ));
    }

    Ok(trimmed.to_string())
}

/// Build install spec from SkillHub API detail response (testable without HTTP).
fn spec_from_skillhub_detail(slug: &str, detail: SkillHubDetailResponse) -> AppResult<InstallSpec> {
    let display_name = detail
        .skill
        .display_name
        .filter(|s| !s.is_empty())
        .or_else(|| Some(detail.skill.slug.clone()));

    if let Some(repo) = detail.repo_url.as_deref().filter(|u| !u.is_empty()) {
        if repo.starts_with("https://") || repo.starts_with("git@") {
            crate::security::ipc_policy::validate_skill_git_url(repo)?;
            return Ok(InstallSpec {
                source: SkillInstallSource::Git,
                path_or_url: repo.to_string(),
                subpath: None,
                display_name,
            });
        }
    }

    let file_url = skillhub_skill_file_url(slug);
    crate::security::ipc_policy::validate_skill_remote_url(&file_url)?;

    Ok(InstallSpec {
        source: SkillInstallSource::Url,
        path_or_url: file_url,
        subpath: None,
        display_name,
    })
}

fn skillhub_skill_file_url(slug: &str) -> String {
    format!(
        "{SKILLHUB_API}/api/v1/skills/{}/file?path=SKILL.md",
        urlencoding::encode(slug)
    )
}

async fn resolve_skillhub(reference: &str) -> AppResult<InstallSpec> {
    let slug = normalize_skillhub_reference(reference)?;

    let client = crate::network::cert_pinning::create_https_client()?;
    let detail_url = format!(
        "{SKILLHUB_API}/api/v1/skills/{}",
        urlencoding::encode(&slug)
    );
    let resp = client
        .get(&detail_url)
        .send()
        .await
        .map_err(|e| AppError::msg(format!("SkillHub 请求失败: {e}")))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(AppError::msg(format!(
            "SkillHub 未找到 skill \"{slug}\"。可在 Skills 面板用 URL 手动安装"
        )));
    }
    if !resp.status().is_success() {
        return Err(AppError::msg(format!("SkillHub HTTP {}", resp.status())));
    }

    let detail: SkillHubDetailResponse = resp
        .json()
        .await
        .map_err(|e| AppError::msg(format!("SkillHub 响应解析失败: {e}")))?;

    spec_from_skillhub_detail(&slug, detail)
}

/// Resolve a registry reference to an install spec.
pub async fn resolve_registry(registry: RegistryId, reference: &str) -> AppResult<InstallSpec> {
    let id = registry.as_str();
    registry_adapter(id)?.resolve(reference).await
}

/// Resolve by registry name string (e.g. `"skillhub"`).
pub async fn resolve_registry_named(registry: &str, reference: &str) -> AppResult<InstallSpec> {
    registry_adapter(registry)?.resolve(reference).await
}

/// Build a preview payload for tool confirmation UI.
pub async fn preview_registry_install(
    registry: &str,
    reference: &str,
) -> AppResult<serde_json::Value> {
    let spec = resolve_registry_named(registry, reference).await?;
    Ok(serde_json::json!({
        "registry": registry,
        "reference": reference,
        "resolved_source": match spec.source {
            SkillInstallSource::Url => "url",
            SkillInstallSource::Git => "git",
            SkillInstallSource::Local => "local",
            SkillInstallSource::Registry => "registry",
        },
        "resolved_url": spec.path_or_url,
        "subpath": spec.subpath,
        "display_name": spec.display_name,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_slug_from_name() {
        assert_eq!(
            normalize_skillhub_reference("scrapling").unwrap(),
            "scrapling"
        );
    }

    #[test]
    fn normalize_slug_from_page_url() {
        assert_eq!(
            normalize_skillhub_reference("https://skillhub.cn/skills/scrapling").unwrap(),
            "scrapling"
        );
    }

    #[test]
    fn reject_install_guide_page() {
        assert!(normalize_skillhub_reference("https://skillhub.cn/install/skillhub.md").is_err());
    }

    #[test]
    fn reject_non_skillhub_domain() {
        assert!(normalize_skillhub_reference("https://example.com/skills/foo").is_err());
    }

    #[test]
    fn registry_id_parse() {
        assert_eq!(RegistryId::parse("skillhub"), Some(RegistryId::SkillHub));
        assert_eq!(RegistryId::parse("SkillHub"), Some(RegistryId::SkillHub));
    }

    #[test]
    fn registry_adapter_map_contains_skillhub_aliases() {
        assert!(REGISTRY_ADAPTERS.contains_key("skillhub"));
        assert!(REGISTRY_ADAPTERS.contains_key("skillhub.cn"));
        assert_eq!(registry_adapter("skillhub").unwrap().id(), "skillhub");
    }

    #[test]
    fn spec_from_detail_prefers_git_repo() {
        let detail = SkillHubDetailResponse {
            skill: SkillHubSkillMeta {
                slug: "scrapling".into(),
                display_name: Some("Scrapling".into()),
            },
            repo_url: Some("https://github.com/example/scrapling.git".into()),
        };
        let spec = spec_from_skillhub_detail("scrapling", detail).unwrap();
        assert_eq!(spec.source, SkillInstallSource::Git);
        assert!(spec.path_or_url.contains("github.com"));
        assert_eq!(spec.display_name.as_deref(), Some("Scrapling"));
    }

    #[test]
    fn spec_from_detail_falls_back_to_skill_md_url() {
        let detail = SkillHubDetailResponse {
            skill: SkillHubSkillMeta {
                slug: "minimal".into(),
                display_name: None,
            },
            repo_url: None,
        };
        let spec = spec_from_skillhub_detail("minimal", detail).unwrap();
        assert_eq!(spec.source, SkillInstallSource::Url);
        assert!(spec.path_or_url.contains("api.skillhub.tencent.com"));
        assert!(spec.path_or_url.contains("SKILL.md"));
    }
}
