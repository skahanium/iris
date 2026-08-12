//! 订阅内容规范化：Feed 解析 → HTML/XHTML → 安全 Markdown → 纯文本/哈希。
//!
//! 流水线（规范 §6.2）：有界字节 → 拒绝 DTD/ENTITY → feed-rs 解析并清理
//! → 选择正文与稳定标识 → htmd 转 Markdown（skip script/style/iframe/form/
//! svg/math）→ 规范化链接/标题/图片 → content_text / SHA-256 →
//! conversion_version。原始源载荷只进 SQLite，永不进 IPC。
//!
//! 阶段 2 Task 2.5 `feed::sync` 将消费本模块；届时移除标注。
#![allow(dead_code)]

use std::sync::LazyLock;

use chrono::{SecondsFormat, Utc};
use regex::Regex;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use crate::feed::model::{ConversionStatus, SourcePayloadKind};

/// 当前转换版本（改转换规则时递增；历史条目按版本重转）。
pub(crate) const FEED_CONVERSION_VERSION: i64 = 1;
/// 标题上限（Unicode scalar）。
const TITLE_MAX_SCALARS: usize = 500;
/// 正文 Markdown 上限（字节）。
const CONTENT_MAX_BYTES: usize = 4 * 1024 * 1024;
/// 无标题条目的确定性占位。
const FALLBACK_TITLE: &str = "（无标题）";

static IMAGE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"!\[([^\]]*)\]\([^)]*\)").expect("image regex"));
static LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?P<image>!?)\[(?P<text>[^\]]*)\]\((?P<url>[^)\s]*)\)").expect("link regex")
});
static CODE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([^`]*)`").expect("code regex"));
static HEADING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#{1,6}\s+").expect("heading regex"));
static LIST_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[-*+]\s+").expect("list regex"));
static ORDERED_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d+\.\s+").expect("ordered regex"));
static HR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:-{3,}|\*{3,}|_{3,})$").expect("hr regex"));
static UUID_FORMAT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")
        .expect("uuid regex")
});

/// 规范化后的订阅源。
#[derive(Debug, Clone)]
pub(crate) struct NormalizedFeed {
    pub title: String,
    pub site_url: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub items: Vec<NormalizedItem>,
}

/// 规范化后的条目（`sync` 负责补 id/source_id/received_at 后入库）。
#[derive(Debug, Clone)]
pub(crate) struct NormalizedItem {
    pub external_key: String,
    pub canonical_url: Option<String>,
    pub title: String,
    pub author_name: Option<String>,
    pub published_at: Option<String>,
    pub source_updated_at: Option<String>,
    pub summary_markdown: String,
    pub content_markdown: String,
    pub content_text: String,
    pub source_payload: String,
    pub source_payload_kind: SourcePayloadKind,
    pub content_hash: String,
    pub conversion_status: ConversionStatus,
}

/// 解析并规范化订阅内容；`source_id` 仅用于稳定键回退（无 ID/链接时）。
pub(crate) fn normalize_feed(bytes: &[u8], source_id: &str) -> AppResult<NormalizedFeed> {
    reject_xml_declarations(bytes)?;
    let feed = feed_rs::parser::parse(bytes).map_err(|_| AppError::msg("feed_parse_failed"))?;
    Ok(NormalizedFeed {
        title: feed
            .title
            .as_ref()
            .map(|title| title.content.clone())
            .unwrap_or_default(),
        site_url: pick_site_url(&feed.links),
        description: feed
            .description
            .as_ref()
            .map(|description| description.content.clone()),
        language: feed.language.clone(),
        items: feed
            .entries
            .iter()
            .map(|entry| normalize_item(entry, source_id))
            .collect(),
    })
}

/// parser 前拒绝 XML 声明（ASCII case-insensitive 匹配 `<!DOCTYPE` / `<!ENTITY`）。
fn reject_xml_declarations(bytes: &[u8]) -> AppResult<()> {
    const DOCTYPE: &[u8] = b"<!doctype";
    const ENTITY: &[u8] = b"<!entity";
    let doctype_hit = bytes
        .windows(DOCTYPE.len())
        .any(|window| window.eq_ignore_ascii_case(DOCTYPE));
    let entity_hit = bytes
        .windows(ENTITY.len())
        .any(|window| window.eq_ignore_ascii_case(ENTITY));
    if doctype_hit || entity_hit {
        return Err(AppError::msg("feed_xml_unsafe_declaration"));
    }
    Ok(())
}

/// 站点 URL：优先 `rel=alternate`，否则第一个链接；仅保留安全 HTTPS。
fn pick_site_url(links: &[feed_rs::model::Link]) -> Option<String> {
    links
        .iter()
        .find(|link| link.rel.as_deref() == Some("alternate"))
        .or_else(|| links.first())
        .map(|link| link.href.clone())
        .and_then(|href| safe_url(&href))
}

/// 仅接受合法 HTTPS URL；其余返回 None。
fn safe_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    crate::network::safe_https::validate_https_url(trimmed)
        .ok()
        .map(|()| trimmed.to_string())
}

fn normalize_item(entry: &feed_rs::model::Entry, source_id: &str) -> NormalizedItem {
    let canonical_url = entry
        .links
        .iter()
        .find(|link| link.rel.as_deref() == Some("alternate"))
        .or_else(|| entry.links.iter().find(|link| link.rel.is_none()))
        .map(|link| link.href.clone())
        .and_then(|href| safe_url(&href));

    let raw_title = entry
        .title
        .as_ref()
        .map(|title| title.content.clone())
        .unwrap_or_default();
    let (title, title_truncated) = truncate_scalars(raw_title, TITLE_MAX_SCALARS);
    let title = if title.trim().is_empty() {
        FALLBACK_TITLE.to_string()
    } else {
        title
    };

    let external_key = if !entry.id.trim().is_empty() && is_trusted_external_id(&entry.id) {
        entry.id.trim().to_string()
    } else if let Some(url) = &canonical_url {
        url.clone()
    } else {
        fallback_key(source_id, &title, entry.published)
    };

    let (payload, payload_kind) = select_payload(entry);
    let (summary_markdown, _) = to_markdown(
        entry
            .summary
            .as_ref()
            .map(|summary| summary.content.as_str())
            .unwrap_or(""),
        payload_kind_of(
            entry
                .summary
                .as_ref()
                .map(|s| s.content_type.to_string())
                .as_deref(),
        ),
    );
    let summary_markdown = rewrite_links(&summary_markdown, canonical_url.as_deref());

    let (content_markdown, mut degraded) = to_markdown(&payload, payload_kind);
    let content_markdown = rewrite_links(&content_markdown, canonical_url.as_deref());
    // 截断预算为上限减 1：末尾换行是输出契约，预留后总长不超过 4 MiB。
    let (content_markdown, content_truncated) =
        truncate_bytes(content_markdown, CONTENT_MAX_BYTES - 1);
    degraded |= content_truncated || title_truncated;
    let content_markdown = ensure_trailing_newline(content_markdown);
    let content_hash = content_hash_of(&content_markdown);
    let content_text = markdown_to_text(&content_markdown);

    NormalizedItem {
        external_key,
        canonical_url,
        title,
        author_name: entry.authors.first().and_then(|author| {
            // feed-rs 对 RSS2 <author> 会把元素标签名填入 name、真实值放入 email。
            let name = author.name.trim();
            if name.is_empty() || matches!(name, "author" | "creator" | "contributor") {
                author.email.clone()
            } else {
                Some(author.name.clone())
            }
        }),
        published_at: entry.published.map(rfc3339),
        source_updated_at: entry.updated.map(rfc3339),
        summary_markdown,
        content_markdown,
        content_text,
        source_payload: payload,
        source_payload_kind: payload_kind,
        content_hash,
        conversion_status: if degraded {
            ConversionStatus::Degraded
        } else {
            ConversionStatus::Ok
        },
    }
}

/// 选择主内容（规范 §6.1）：非空 content.body → summary.content →
/// 标题与规范链接的最小 Markdown。未知/矛盾内容类型按纯文本安全降级。
fn select_payload(entry: &feed_rs::model::Entry) -> (String, SourcePayloadKind) {
    if let Some(content) = &entry.content {
        if let Some(body) = content
            .body
            .as_deref()
            .filter(|body| !body.trim().is_empty())
        {
            return (
                body.to_string(),
                payload_kind_of(Some(content.content_type.as_ref())),
            );
        }
    }
    if let Some(summary) = &entry.summary {
        if !summary.content.trim().is_empty() {
            return (
                summary.content.clone(),
                payload_kind_of(Some(summary.content_type.as_ref())),
            );
        }
    }
    let minimal = match &entry.title {
        Some(title) if !title.content.trim().is_empty() => {
            let link = entry
                .links
                .iter()
                .find(|link| link.rel.as_deref() == Some("alternate"))
                .or_else(|| entry.links.first())
                .map(|link| link.href.clone())
                .unwrap_or_default();
            if link.trim().is_empty() {
                title.content.clone()
            } else {
                format!("{} {link}", title.content)
            }
        }
        _ => FALLBACK_TITLE.to_string(),
    };
    (minimal, SourcePayloadKind::Text)
}

/// 内容类型 → 载荷种类；未知或矛盾一律纯文本安全降级。
fn payload_kind_of(content_type: Option<&str>) -> SourcePayloadKind {
    let raw = content_type.unwrap_or("").to_ascii_lowercase();
    if raw.contains("xhtml") {
        SourcePayloadKind::Xhtml
    } else if raw.contains("html") {
        SourcePayloadKind::Html
    } else if raw.contains("markdown") {
        SourcePayloadKind::Markdown
    } else {
        SourcePayloadKind::Text
    }
}

/// 载荷 → 规范化 Markdown；HTML/XHTML 经 htmd（skip 危险节点），
/// 纯文本转义，Markdown 原样；转换失败按转义纯文本降级。
fn to_markdown(payload: &str, kind: SourcePayloadKind) -> (String, bool) {
    match kind {
        SourcePayloadKind::Html | SourcePayloadKind::Xhtml => match html_to_markdown(payload) {
            Ok(markdown) => (markdown, false),
            Err(_) => (escape_plain_text(payload), true),
        },
        SourcePayloadKind::Markdown => (payload.to_string(), false),
        SourcePayloadKind::Text => (escape_plain_text(payload), false),
    }
}

fn html_to_markdown(html: &str) -> std::io::Result<String> {
    htmd::HtmlToMarkdown::builder()
        .skip_tags(vec!["script", "style", "iframe", "form", "svg", "math"])
        .build()
        .convert(html)
}

/// 纯文本转义为安全 Markdown（特殊字符前加反斜杠，确定性规则）。
fn escape_plain_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(
            ch,
            '\\' | '`' | '*' | '_' | '[' | ']' | '#' | '>' | '|' | '~'
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// 规范化 Markdown 链接与图片：相对链接只以文章 HTTPS URL 为基准；
/// 不安全 URL（javascript:/data:/file:/asset:/mailto: 等）转纯文本。
fn rewrite_links(markdown: &str, base: Option<&str>) -> String {
    let link_re = regex::Regex::new(r"(?P<image>!?)\[(?P<text>[^\]]*)\]\((?P<url>[^)\s]*)\)")
        .expect("link regex");
    link_re
        .replace_all(markdown, |captures: &regex::Captures<'_>| {
            let is_image = captures.name("image").is_some();
            let text = captures.name("text").map(|m| m.as_str()).unwrap_or("");
            let url = captures
                .name("url")
                .map(|m| m.as_str())
                .unwrap_or("")
                .trim();
            let rewritten = resolve_link_url(url, base);
            match rewritten {
                Some(absolute) if is_image => format!("![{text}]({absolute})"),
                Some(absolute) => format!("[{text}]({absolute})"),
                None if is_image => text.to_string(),
                None => text.to_string(),
            }
        })
        .into_owned()
}

/// 返回可安全进入 Markdown 的绝对 URL；无法安全解析则返回 None（纯文本）。
fn resolve_link_url(url: &str, base: Option<&str>) -> Option<String> {
    if url.is_empty() {
        return None;
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        return safe_url(url);
    }
    if url.contains("://") {
        // 其他 scheme（javascript:、data:、file:、asset:、mailto: 等）。
        return None;
    }
    // 相对链接：只以安全的文章 HTTPS URL 为基准。
    let base = base.and_then(safe_url)?;
    let joined = reqwest::Url::parse(&base).ok()?.join(url).ok()?;
    safe_url(joined.as_str())
}

/// content_text：从最终 Markdown 确定性去标记，不使用浏览器 DOM。
fn markdown_to_text(markdown: &str) -> String {
    let mut out = String::new();
    let mut in_code_fence = false;
    for raw_line in markdown.lines() {
        let mut line = raw_line.trim().to_string();
        if line.starts_with("```") {
            in_code_fence = !in_code_fence;
            continue;
        }
        if !in_code_fence {
            // 单行 HTML 注释
            if let Some(start) = line.find("<!--") {
                if let Some(end) = line[start..].find("-->") {
                    line.replace_range(start..start + end + 3, "");
                }
            }
            // 图片与链接 → 文本
            line = IMAGE_RE
                .replace_all(&line, |captures: &regex::Captures<'_>| {
                    captures
                        .get(1)
                        .map(|m| m.as_str())
                        .unwrap_or("")
                        .to_string()
                })
                .into_owned();
            line = LINK_RE
                .replace_all(&line, |captures: &regex::Captures<'_>| {
                    captures
                        .get(1)
                        .map(|m| m.as_str())
                        .unwrap_or("")
                        .to_string()
                })
                .into_owned();
            // 行内代码与强调标记（代码先剥除，避免误伤）
            line = CODE_RE.replace_all(&line, "$1").into_owned();
            line.retain(|ch| !matches!(ch, '*' | '_' | '~'));
            // 标题、引用、列表标记与分隔线
            if line.starts_with('#') {
                line = HEADING_RE.replace(&line, "").into_owned();
            } else if let Some(rest) = line.strip_prefix('>') {
                line = rest.trim().to_string();
            } else if LIST_RE.is_match(&line) {
                line = LIST_RE.replace(&line, "").into_owned();
            } else if ORDERED_RE.is_match(&line) {
                line = ORDERED_RE.replace(&line, "").into_owned();
            } else if HR_RE.is_match(&line) {
                continue;
            }
        }
        out.push_str(&line);
        out.push(' ');
    }
    collapse_whitespace(&out)
}

fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

fn rfc3339(value: chrono::DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn content_hash_of(markdown: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(markdown.as_bytes());
    hex::encode(hasher.finalize())
}

/// 按 Unicode scalar 截断（天然不切坏 UTF-8）。
fn truncate_scalars(text: String, max: usize) -> (String, bool) {
    if text.chars().count() <= max {
        return (text, false);
    }
    (text.chars().take(max).collect(), true)
}

/// 按字节上限截断，落在字符边界。
fn truncate_bytes(text: String, max: usize) -> (String, bool) {
    if text.len() <= max {
        return (text, false);
    }
    let mut out = String::with_capacity(max);
    for ch in text.chars() {
        if out.len() + ch.len_utf8() > max {
            break;
        }
        out.push(ch);
    }
    (out, true)
}

fn ensure_trailing_newline(mut text: String) -> String {
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

/// 裸 UUID 格式的 id 是 feed-rs 为缺失 id 注入的随机值（每次解析不同），
/// 不能作为稳定键；`urn:uuid:` 等带前缀的真实 id 不受影响。
fn is_trusted_external_id(id: &str) -> bool {
    !UUID_FORMAT_RE.is_match(id.trim())
}

/// 稳定键回退：`source_id + title + published_at` 的 SHA-256。
fn fallback_key(source_id: &str, title: &str, published: Option<chrono::DateTime<Utc>>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(title.as_bytes());
    hasher.update(b"\0");
    hasher.update(published.map(rfc3339).unwrap_or_default().as_bytes());
    hex::encode(hasher.finalize())
}
