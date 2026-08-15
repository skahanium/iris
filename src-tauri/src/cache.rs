//! Iris 缓存治理：统一统计、可安全清理范围与运行时资源修复标记。
//!
//! 此模块不把不同业务缓存混为一体；它只集中管理用户可见的生命周期边界。

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use chrono::{SecondsFormat, Utc};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::error::AppResult;
use crate::paths::IrisPaths;
use crate::storage::db::Database;

const RUNTIME_REPAIR_SETTING: &str = "cache_runtime_repair_pending";
const LAST_MAINTENANCE_SETTING: &str = "cache_maintenance_at";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CacheDomainId {
    FeedMedia,
    WebPages,
    TemporaryFiles,
    RuntimeArtifacts,
    UpdatePackages,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheDomainSummary {
    pub id: CacheDomainId,
    pub label: String,
    pub bytes: u64,
    pub entries: u64,
    pub reclaimable_bytes: u64,
    pub active_bytes: u64,
    pub clearable: bool,
    pub policy: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheSummary {
    pub domains: Vec<CacheDomainSummary>,
    pub total_bytes: u64,
    pub reclaimable_bytes: u64,
    pub generated_at: String,
    pub last_maintenance_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheClearRequest {
    pub domains: Vec<CacheDomainId>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheClearDomainResult {
    pub id: CacheDomainId,
    pub bytes_freed: u64,
    pub entries_removed: u64,
    pub skipped_active: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheClearResult {
    pub domains: Vec<CacheClearDomainResult>,
    pub completed_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCacheRepairRequest {
    pub domains: Vec<RuntimeCacheDomain>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCacheDomain {
    Ort,
    Huggingface,
    Xdg,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRepairResult {
    pub restart_required: bool,
    pub domains: Vec<RuntimeCacheDomain>,
}

pub struct CacheCoordinator<'a> {
    paths: &'a IrisPaths,
    db: &'a Database,
}

impl<'a> CacheCoordinator<'a> {
    pub fn new(paths: &'a IrisPaths, db: &'a Database) -> Self {
        Self { paths, db }
    }

    pub fn summary(&self) -> AppResult<CacheSummary> {
        let feed = directory_usage(&self.paths.cache_dir.join("feed-media"));
        let temp = directory_usage(&self.paths.temp_dir);
        let runtime = runtime_usage(&self.paths.cache_dir);
        let updates = directory_usage(&self.paths.cache_dir.join("updates"));
        let (web_entries, web_bytes): (i64, i64) = self.db.with_read_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(length(url) + COALESCE(length(title), 0) + length(body_text)), 0)
                 FROM web_page_cache",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(Into::into)
        })?;
        let domains = vec![
            disk_summary(
                CacheDomainId::FeedMedia,
                "RSS 媒体缓存",
                feed,
                true,
                "图片与 PDF 合计 1 GiB；30 天未访问淘汰。",
            ),
            CacheDomainSummary {
                id: CacheDomainId::WebPages,
                label: "网页正文缓存".into(),
                bytes: web_bytes.max(0) as u64,
                entries: web_entries.max(0) as u64,
                reclaimable_bytes: web_bytes.max(0) as u64,
                active_bytes: 0,
                clearable: true,
                policy: "24 小时；最多 256 条和 16 MiB，按最近访问淘汰。".into(),
            },
            disk_summary(
                CacheDomainId::TemporaryFiles,
                "临时文件",
                temp,
                true,
                "非活动文件保留 7 天；最多 512 MiB。",
            ),
            disk_summary(
                CacheDomainId::RuntimeArtifacts,
                "运行时资源",
                runtime,
                false,
                "仅统计；通过“修复运行时资源”在下次重启时重建。",
            ),
            disk_summary(
                CacheDomainId::UpdatePackages,
                "更新包",
                updates,
                false,
                "当前更新包由更新器校验与回收，不参与普通清理。",
            ),
        ];
        let total_bytes = domains.iter().map(|item| item.bytes).sum();
        let reclaimable_bytes = domains.iter().map(|item| item.reclaimable_bytes).sum();
        Ok(CacheSummary {
            domains,
            total_bytes,
            reclaimable_bytes,
            generated_at: now(),
            last_maintenance_at: self.last_maintenance_at()?,
        })
    }

    pub fn clear(&self, request: CacheClearRequest) -> AppResult<CacheClearResult> {
        let mut results = Vec::new();
        for domain in request.domains {
            let cleared = match domain {
                CacheDomainId::FeedMedia => self.clear_feed_media(),
                CacheDomainId::WebPages => self.clear_web_pages(),
                CacheDomainId::TemporaryFiles => self.clear_temporary_files(),
                CacheDomainId::RuntimeArtifacts | CacheDomainId::UpdatePackages => {
                    Ok(CacheClearDomainResult {
                        id: domain,
                        bytes_freed: 0,
                        entries_removed: 0,
                        skipped_active: 0,
                        error: None,
                    })
                }
            };
            results.push(match cleared {
                Ok(result) => result,
                Err(error) => CacheClearDomainResult {
                    id: domain,
                    bytes_freed: 0,
                    entries_removed: 0,
                    skipped_active: 0,
                    error: Some(error.to_string()),
                },
            });
        }
        self.record_maintenance()?;
        Ok(CacheClearResult {
            domains: results,
            completed_at: now(),
        })
    }

    pub fn prepare_runtime_repair(
        &self,
        request: RuntimeCacheRepairRequest,
    ) -> AppResult<RuntimeRepairResult> {
        let value = serde_json::to_string(&request.domains)?;
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![RUNTIME_REPAIR_SETTING, value],
            )?;
            Ok(())
        })?;
        Ok(RuntimeRepairResult {
            restart_required: true,
            domains: request.domains,
        })
    }

    fn clear_feed_media(&self) -> AppResult<CacheClearDomainResult> {
        let root = self.paths.cache_dir.join("feed-media");
        let before = directory_usage(&root);
        let images = crate::feed::image::clear_cache(&root.join("images"))?;
        let documents = crate::feed::document::clear_cache(&root.join("documents"))?;
        let after = directory_usage(&root);
        Ok(CacheClearDomainResult {
            id: CacheDomainId::FeedMedia,
            bytes_freed: before.bytes.saturating_sub(after.bytes),
            entries_removed: u64::from(images + documents),
            skipped_active: 0,
            error: None,
        })
    }

    fn clear_web_pages(&self) -> AppResult<CacheClearDomainResult> {
        let (bytes, entries): (i64, i64) = self.db.with_conn(|conn| {
            let summary = conn.query_row(
                "SELECT COALESCE(SUM(length(url) + COALESCE(length(title), 0) + length(body_text)), 0), COUNT(*)
                 FROM web_page_cache",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            conn.execute("DELETE FROM web_page_cache", [])?;
            Ok(summary)
        })?;
        Ok(CacheClearDomainResult {
            id: CacheDomainId::WebPages,
            bytes_freed: bytes.max(0) as u64,
            entries_removed: entries.max(0) as u64,
            skipped_active: 0,
            error: None,
        })
    }

    fn clear_temporary_files(&self) -> AppResult<CacheClearDomainResult> {
        crate::temp_files::ensure_owned(&self.paths.temp_dir)?;
        let sweep = crate::temp_files::sweep(
            &self.paths.temp_dir,
            &crate::temp_files::TempSweepConfig {
                now: SystemTime::now(),
                max_age: crate::temp_files::DEFAULT_TEMP_MAX_AGE,
                max_bytes: crate::temp_files::DEFAULT_TEMP_MAX_BYTES,
                cap_min_age: crate::temp_files::TEMP_CAP_MIN_AGE,
                secure_delete: true,
                modified_time: &|path| {
                    std::fs::metadata(path)
                        .and_then(|metadata| metadata.modified())
                        .unwrap_or_else(|_| SystemTime::now())
                },
            },
        )?;
        Ok(CacheClearDomainResult {
            id: CacheDomainId::TemporaryFiles,
            bytes_freed: sweep.freed_bytes,
            entries_removed: sweep.deleted_files as u64,
            skipped_active: sweep.skipped_active as u64,
            error: None,
        })
    }

    fn record_maintenance(&self) -> AppResult<()> {
        let value = serde_json::to_string(&now())?;
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![LAST_MAINTENANCE_SETTING, value],
            )?;
            Ok(())
        })
    }

    fn last_maintenance_at(&self) -> AppResult<Option<String>> {
        self.db.with_read_conn(|conn| {
            let value: Option<String> = conn
                .query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    [LAST_MAINTENANCE_SETTING],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(value.and_then(|item| serde_json::from_str(&item).ok()))
        })
    }
}

/// Apply a user-requested runtime repair before engines initialize.
pub fn apply_pending_runtime_repair(paths: &IrisPaths, db: &Database) -> AppResult<()> {
    let pending: Option<String> = db.with_read_conn(|conn| {
        use rusqlite::OptionalExtension;
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            [RUNTIME_REPAIR_SETTING],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
    })?;
    let Some(pending) = pending else {
        return Ok(());
    };
    let domains: Vec<RuntimeCacheDomain> = serde_json::from_str(&pending).unwrap_or_default();
    for domain in domains {
        let path = match domain {
            RuntimeCacheDomain::Ort => paths.cache_dir.join("ort"),
            RuntimeCacheDomain::Huggingface => paths.cache_dir.join("huggingface"),
            RuntimeCacheDomain::Xdg => paths.cache_dir.join("xdg"),
        };
        remove_children(&path)?;
    }
    db.with_conn(|conn| {
        conn.execute(
            "DELETE FROM settings WHERE key = ?1",
            [RUNTIME_REPAIR_SETTING],
        )?;
        Ok(())
    })
}

/// Move legacy RSS cache files out of the application-state directory once.
/// Incomplete downloads are intentionally discarded: they have no verified
/// content and cannot be resumed safely across the path change.
pub fn migrate_legacy_feed_cache(paths: &IrisPaths) -> AppResult<()> {
    for (legacy, destination) in [
        (
            paths.data_dir.join("cache").join("feed-images"),
            paths.cache_dir.join("feed-media").join("images"),
        ),
        (
            paths.data_dir.join("cache").join("feed-documents"),
            paths.cache_dir.join("feed-media").join("documents"),
        ),
    ] {
        if !legacy.is_dir() {
            continue;
        }
        fs::create_dir_all(&destination)?;
        for entry in fs::read_dir(&legacy)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path
                .extension()
                .is_some_and(|extension| extension == "part")
            {
                let _ = fs::remove_file(path);
                continue;
            }
            let target = destination.join(entry.file_name());
            if target.exists() {
                continue;
            }
            fs::rename(path, target)?;
        }
        if fs::read_dir(&legacy)?.next().is_none() {
            let _ = fs::remove_dir_all(&legacy);
            if let Some(parent) = legacy.parent().filter(|parent| parent != &paths.data_dir) {
                if fs::read_dir(parent).is_ok_and(|mut entries| entries.next().is_none()) {
                    let _ = fs::remove_dir(parent);
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Default)]
struct DirectoryUsage {
    bytes: u64,
    entries: u64,
}

fn directory_usage(root: &Path) -> DirectoryUsage {
    if !root.is_dir() {
        return DirectoryUsage::default();
    }
    WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .fold(DirectoryUsage::default(), |mut usage, metadata| {
            usage.bytes = usage.bytes.saturating_add(metadata.len());
            usage.entries += 1;
            usage
        })
}

fn runtime_usage(cache_dir: &Path) -> DirectoryUsage {
    ["ort", "huggingface", "xdg"]
        .into_iter()
        .map(|name| directory_usage(&cache_dir.join(name)))
        .fold(DirectoryUsage::default(), |mut total, usage| {
            total.bytes = total.bytes.saturating_add(usage.bytes);
            total.entries += usage.entries;
            total
        })
}

fn disk_summary(
    id: CacheDomainId,
    label: &str,
    usage: DirectoryUsage,
    clearable: bool,
    policy: &str,
) -> CacheDomainSummary {
    CacheDomainSummary {
        id,
        label: label.into(),
        bytes: usage.bytes,
        entries: usage.entries,
        reclaimable_bytes: if clearable { usage.bytes } else { 0 },
        active_bytes: 0,
        clearable,
        policy: policy.into(),
    }
}

fn remove_children(root: &Path) -> AppResult<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if fs::symlink_metadata(&path)?.file_type().is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_paths(root: &Path) -> IrisPaths {
        IrisPaths {
            home_dir: root.to_path_buf(),
            data_dir: root.join("data"),
            cache_dir: root.join("cache"),
            temp_dir: root.join("tmp"),
            global_skills_dir: root.join("skills"),
            temp_dir_explicit: false,
        }
    }

    #[test]
    fn clearing_feed_media_never_touches_runtime_artifacts() {
        let root = tempfile::tempdir().unwrap();
        let paths = test_paths(root.path());
        fs::create_dir_all(paths.cache_dir.join("feed-media/images")).unwrap();
        fs::create_dir_all(paths.cache_dir.join("ort")).unwrap();
        fs::write(paths.cache_dir.join("feed-media/images/a.png"), b"feed").unwrap();
        fs::write(paths.cache_dir.join("ort/model.bin"), b"runtime").unwrap();
        let db = Database::open_in_memory().unwrap();
        let coordinator = CacheCoordinator::new(&paths, &db);

        let result = coordinator
            .clear(CacheClearRequest {
                domains: vec![CacheDomainId::FeedMedia],
            })
            .unwrap();

        assert_eq!(result.domains[0].entries_removed, 1);
        assert!(!paths.cache_dir.join("feed-media/images/a.png").exists());
        assert!(paths.cache_dir.join("ort/model.bin").exists());
    }

    #[test]
    fn legacy_rss_cache_moves_verified_files_and_discards_partials() {
        let root = tempfile::tempdir().unwrap();
        let paths = test_paths(root.path());
        let legacy = paths.data_dir.join("cache/feed-images");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("ready.png"), b"ready").unwrap();
        fs::write(legacy.join("interrupted.part"), b"partial").unwrap();

        migrate_legacy_feed_cache(&paths).unwrap();

        assert!(paths.cache_dir.join("feed-media/images/ready.png").exists());
        assert!(!legacy.join("interrupted.part").exists());
    }
}
