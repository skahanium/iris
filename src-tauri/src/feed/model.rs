//! 订阅资料库领域类型与 IPC DTO。
//!
//! 所有对外 DTO 使用 `camelCase`（配合 `#[serde(rename_all = "camelCase")]`），
//! 与 `src/types/ipc.ts` 保持一致；`source_payload` 永不进入任何 DTO。

use serde::{Deserialize, Serialize};

/// 订阅源列表摘要（IPC 使用 camelCase）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedSourceSummary {
    pub id: String,
    pub title: String,
    pub feed_url: String,
    pub site_url: Option<String>,
    pub folder_path: String,
    pub is_enabled: bool,
    pub fetch_interval_minutes: i64,
    /// 是否对该来源摘要条目自动启用通用网页正文补全；默认开启。
    pub fulltext_enabled: bool,
    pub unread_count: i64,
    pub last_checked_at: Option<String>,
    pub last_success_at: Option<String>,
    pub next_fetch_at: Option<String>,
    pub consecutive_failures: i64,
    pub last_error_code: Option<String>,
}

/// 订阅资料库全局维护摘要；不包含 URL、正文或错误详情。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeedLibrarySummary {
    pub source_count: i64,
    pub enabled_source_count: i64,
    pub failed_source_count: i64,
    pub item_count: i64,
    pub unread_count: i64,
    pub last_success_at: Option<String>,
}

/// 文章视图（snake_case 序列化，与前端 `FeedView` 字符串一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedView {
    Inbox,
    Today,
    All,
    Starred,
    Archived,
}

/// 稳定 keyset 游标：按 `(sort_at DESC, row_id DESC)` 排序，其中 `sort_at`
/// 是 `published_at` 缺失时回退到 `received_at` 的展示排序时间。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedPageCursor {
    pub sort_at: String,
    pub row_id: i64,
}

/// 冻结的文章查询条件（批量已读与列表共用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedItemQuery {
    pub view: FeedView,
    pub source_id: Option<String>,
    /// 有界全文检索词；列表、分页与批量操作共用同一冻结条件。
    #[serde(default)]
    pub search: Option<String>,
    pub received_after: Option<String>,
    pub cursor: Option<FeedPageCursor>,
    pub limit: u32,
}

/// 文章列表摘要（IPC 使用 camelCase）。
///
/// `row_id` 是排序并列时的稳定决胜键，前端用它构造 [`FeedPageCursor`]。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedItemSummary {
    pub row_id: i64,
    pub id: String,
    pub source_id: String,
    pub source_title: String,
    pub title: String,
    pub author_name: Option<String>,
    pub canonical_url: Option<String>,
    pub published_at: Option<String>,
    pub received_at: String,
    pub sort_at: String,
    pub excerpt: String,
    pub is_read: bool,
    pub is_starred: bool,
    pub is_archived: bool,
    pub conversion_status: String,
}

/// 文章详情：只包含规范化 Markdown 与安全元数据，无 `source_payload`。
/// `site_url` 为订阅源站点地址（保存为笔记的「来源」链接用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedItemDetail {
    pub summary: FeedItemSummary,
    pub content_markdown: String,
    pub summary_markdown: String,
    pub site_url: Option<String>,
    pub content_origin: String,
    pub fulltext_status: String,
    pub primary_document: Option<FeedPrimaryDocument>,
    pub fulltext_needs_refresh: bool,
    /// 仅当前文章的显式图片加载授权；不代表来源级或全局授权。
    pub images_authorized: bool,
}

/// 本地受控图片 lease；不暴露缓存路径。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeedImageLease {
    pub source_url: String,
    pub handle: String,
    pub url: String,
    pub mime_type: String,
    pub size_bytes: u64,
}

/// 用户授权后可逐张请求的图片清单；只含 Markdown 中已有的源地址与稳定索引。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeedImageManifest {
    pub images: Vec<FeedImageSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeedImageSource {
    pub index: u32,
    pub source_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeedPrimaryDocument {
    pub kind: String,
    pub url: String,
}

/// 单篇网页正文补全请求的稳定结果；调用方无需根据底层数据库状态推断。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedFulltextEnqueueOutcome {
    Queued,
    AlreadyQueued,
    AlreadyReady,
    NotEligible,
}

impl FeedFulltextEnqueueOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::AlreadyQueued => "already_queued",
            Self::AlreadyReady => "already_ready",
            Self::NotEligible => "not_eligible",
        }
    }
}

/// RSS 专属回收站条目；与 Markdown 文件回收站的生命周期互不关联。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedTrashItem {
    pub item: FeedItemSummary,
    pub deleted_at: String,
    pub purge_after: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeedTrashSource {
    pub id: String,
    pub title: String,
    pub item_count: i64,
    pub starred_count: i64,
    pub deleted_at: String,
    pub purge_after: String,
}

/// 退订确认所需的有界统计；不包含 URL、正文或缓存路径。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeedSourceTrashPreview {
    pub item_count: i64,
    pub starred_count: i64,
    pub purge_after: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedTrashSnapshot {
    pub sources: Vec<FeedTrashSource>,
    pub items: Vec<FeedTrashItem>,
}

/// 条目级源载荷种类（对应 `feed_items.source_payload_kind` CHECK 约束）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourcePayloadKind {
    Html,
    Xhtml,
    Text,
    Markdown,
}

impl SourcePayloadKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Xhtml => "xhtml",
            Self::Text => "text",
            Self::Markdown => "markdown",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "html" => Some(Self::Html),
            "xhtml" => Some(Self::Xhtml),
            "text" => Some(Self::Text),
            "markdown" => Some(Self::Markdown),
            _ => None,
        }
    }
}

/// 转换状态（对应 `feed_items.conversion_status` CHECK 约束）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversionStatus {
    Ok,
    Degraded,
}

impl ConversionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Degraded => "degraded",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ok" => Some(Self::Ok),
            "degraded" => Some(Self::Degraded),
            _ => None,
        }
    }
}

/// 订阅源完整记录（仓储与同步使用；`title` 为 feed 原标题，展示用
/// `COALESCE(title_override, title)`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedSource {
    pub id: String,
    pub feed_url: String,
    pub site_url: Option<String>,
    pub title: String,
    pub title_override: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub language: Option<String>,
    pub folder_path: String,
    pub is_enabled: bool,
    pub fetch_interval_minutes: i64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub last_checked_at: Option<String>,
    pub last_success_at: Option<String>,
    pub next_fetch_at: Option<String>,
    pub consecutive_failures: i64,
    pub last_error_code: Option<String>,
    pub last_error_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub history_boundary_external_key: Option<String>,
    pub history_boundary_published_at: Option<String>,
    pub fulltext_enabled: bool,
}

/// 新建订阅源输入。
#[derive(Debug, Clone)]
pub struct NewFeedSource {
    pub id: String,
    pub feed_url: String,
    pub site_url: Option<String>,
    pub title: String,
    pub title_override: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub language: Option<String>,
    pub folder_path: String,
    pub fetch_interval_minutes: i64,
}

/// 订阅源可编辑字段补丁；`None` 表示不改动。
#[derive(Debug, Clone, Default)]
pub struct FeedSourcePatch {
    /// 设置覆盖标题。
    pub title_override: Option<String>,
    /// 清除覆盖标题（恢复 feed 原标题），优先于 `title_override`。
    pub clear_title_override: bool,
    pub folder_path: Option<String>,
    pub fetch_interval_minutes: Option<i64>,
    pub is_enabled: Option<bool>,
    /// 来源级网页正文补全开关；未提供时不改变原设置。
    pub fulltext_enabled: Option<bool>,
}

/// 文章阅读状态补丁；至少一个字段必须为 `Some`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedItemStatePatch {
    pub is_read: Option<bool>,
    pub is_starred: Option<bool>,
    pub is_archived: Option<bool>,
}

impl FeedItemStatePatch {
    pub fn is_empty(&self) -> bool {
        self.is_read.is_none() && self.is_starred.is_none() && self.is_archived.is_none()
    }
}

/// 规范化后的条目（阶段 2 normalize 的输出，仓储 upsert 的输入）。
#[derive(Debug, Clone)]
pub struct FeedItemInput {
    pub id: String,
    pub source_id: String,
    pub external_key: String,
    pub canonical_url: Option<String>,
    pub title: String,
    pub author_name: Option<String>,
    pub published_at: Option<String>,
    pub source_updated_at: Option<String>,
    pub received_at: String,
    pub summary_markdown: String,
    pub content_markdown: String,
    pub content_text: String,
    pub source_payload: String,
    pub source_payload_kind: SourcePayloadKind,
    pub content_hash: String,
    pub conversion_version: i64,
    pub conversion_status: ConversionStatus,
    pub expires_at: String,
    pub fulltext_status: FulltextStatus,
}

/// 网页正文缓存状态；`pending` 只由后台受限队列消费。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FulltextStatus {
    NotRequested,
    Pending,
    Fetching,
    Ready,
    Failed,
}

impl FulltextStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotRequested => "not_requested",
            Self::Pending => "pending",
            Self::Fetching => "fetching",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }
}

/// 一次批量 upsert 的结果计数（用于同步事件投影）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpsertSummary {
    pub inserted: usize,
    pub updated: usize,
    pub unchanged: usize,
}

/// 同步结果后的订阅源状态（全量覆盖同步状态列）。
#[derive(Debug, Clone)]
pub struct FeedSourceSyncState {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub last_checked_at: String,
    pub last_success_at: Option<String>,
    pub next_fetch_at: String,
    pub consecutive_failures: i64,
    pub last_error_code: Option<String>,
    pub last_error_at: Option<String>,
}
