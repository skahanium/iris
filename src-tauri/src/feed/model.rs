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
    pub unread_count: i64,
    pub last_checked_at: Option<String>,
    pub last_success_at: Option<String>,
    pub next_fetch_at: Option<String>,
    pub consecutive_failures: i64,
    pub last_error_code: Option<String>,
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

/// 稳定 keyset 游标：按 `(received_at DESC, row_id DESC)` 排序。
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
    pub excerpt: String,
    pub is_read: bool,
    pub is_starred: bool,
    pub is_archived: bool,
    pub conversion_status: String,
}

/// 文章详情：只包含规范化 Markdown 与安全元数据，无 `source_payload`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedItemDetail {
    pub summary: FeedItemSummary,
    pub content_markdown: String,
    pub summary_markdown: String,
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
    // 阶段 3 订阅源详情/编辑消费；届时移除标注。
    #[allow(dead_code)]
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
