//! OPML 导入导出（阶段 5）：订阅关系迁移的开放格式。
//!
//! 输入经 IPC 传有界 UTF-8 字符串（命令层校验 5 MiB 上限），Rust 命令不接收
//! 任意文件路径；解析前复用 `normalize::reject_xml_declarations` 拒绝
//! DTD/ENTITY，只读取 `outline[text/title/xmlUrl/htmlUrl]`。导入只更新
//! `folder_path`/`title_override`，绝不重置同步与阅读状态；导出按
//! `folder_path` 稳定排序为嵌套大纲，不含 ETag、错误、阅读状态或本地 ID。

use std::collections::{BTreeMap, HashSet};

use chrono::Utc;
use quick_xml::events::Event;
use quick_xml::Reader;
use rusqlite::Connection;

use crate::error::{AppError, AppResult};
use crate::feed::model::{FeedSourcePatch, FeedSourceSummary, NewFeedSource};
use crate::feed::normalize::{reject_xml_declarations, TITLE_MAX_SCALARS};
use crate::feed::repository::FeedRepository;
use crate::network::safe_https::validate_https_url;

/// OPML 输入上限（字节），与 Feed 载荷上限一致（规范 §11.1）。
pub const OPML_MAX_BYTES: usize = 5 * 1024 * 1024;

/// 单个订阅大纲（导出→导入的中间表示）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpmlOutline {
    /// 显示标题（截断到 `TITLE_MAX_SCALARS`；缺省用 `xml_url`）。
    pub title: String,
    /// 规范化后的 HTTPS Feed URL；`None` 表示该大纲不可订阅（导入时跳过）。
    pub xml_url: Option<String>,
    pub html_url: Option<String>,
    /// 从嵌套分组推导的 `folder_path`（空串 = 未分组）。
    pub folder_path: String,
}

/// 导入结果计数（camelCase 序列化给前端预览/回执）。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpmlImportResult {
    /// 实际新增的订阅数。
    pub added: u32,
    /// 已存在且 folder/override 有变化的订阅数。
    pub updated: u32,
    /// 无有效 URL、重复 URL 或无变化的订阅数。
    pub skipped: u32,
    /// 新增订阅的稳定 ID（前端按需发起首次同步）。
    pub added_ids: Vec<String>,
}

/// 规范化 HTTPS URL；非法（非 HTTPS/私网等）返回 `None`。
fn safe_https_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    validate_https_url(trimmed)
        .ok()
        .map(|()| trimmed.to_string())
}

/// 标题截断到 `TITLE_MAX_SCALARS`（Unicode scalar），天然不切坏 UTF-8。
fn truncate_title(title: &str) -> String {
    title.chars().take(TITLE_MAX_SCALARS).collect()
}

/// 解析 OPML 字节流；分组 outline（无 `xmlUrl`）只贡献 `folder_path`，
/// 订阅 outline 保留合法 HTTPS URL，非法 URL 归为 `xml_url = None`。
pub fn parse_opml(bytes: &[u8]) -> AppResult<Vec<OpmlOutline>> {
    reject_xml_declarations(bytes)?;
    // OPML 输入契约是有界 UTF-8 字符串：整体先验一次，非法序列直接拒绝，
    // 避免解码损失字符后静默损坏订阅标题。
    std::str::from_utf8(bytes).map_err(|_| AppError::msg("feed_opml_parse_failed"))?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut outlines = Vec::new();
    // 分组栈：每个大纲元素压入一个占位（分组压段名，订阅压空串），End 时弹出。
    let mut folder_stack: Vec<String> = Vec::new();
    // 全元素深度：Eof 时未归零说明文档未闭合（畸形 XML 拒绝）。
    let mut depth: usize = 0;
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element)) => {
                depth += 1;
                if element.name().as_ref() == b"outline" {
                    handle_outline(&reader, &element, true, &mut folder_stack, &mut outlines)?;
                }
            }
            Ok(Event::Empty(element)) => {
                // 自闭合大纲没有 End 事件：订阅产出但不压占位；空分组忽略。
                if element.name().as_ref() == b"outline" {
                    handle_outline(&reader, &element, false, &mut folder_stack, &mut outlines)?;
                }
            }
            Ok(Event::End(element)) => {
                depth = depth.saturating_sub(1);
                if element.name().as_ref() == b"outline" {
                    folder_stack.pop();
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err(AppError::msg("feed_opml_parse_failed")),
            _ => {}
        }
        buffer.clear();
    }
    if depth != 0 {
        return Err(AppError::msg("feed_opml_parse_failed"));
    }
    Ok(outlines)
}

/// 处理单个 `<outline>` 元素；`is_start` 为 false 表示自闭合（无 End 事件）。
fn handle_outline(
    reader: &Reader<&[u8]>,
    element: &quick_xml::events::BytesStart<'_>,
    is_start: bool,
    folder_stack: &mut Vec<String>,
    outlines: &mut Vec<OpmlOutline>,
) -> AppResult<()> {
    let mut text: Option<String> = None;
    let mut xml_url: Option<String> = None;
    let mut html_url: Option<String> = None;
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| AppError::msg("feed_opml_parse_failed"))?;
        let value = attribute
            .decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, reader.decoder())
            .map_err(|_| AppError::msg("feed_opml_parse_failed"))?
            .into_owned();
        match attribute.key.as_ref() {
            b"text" | b"title" if text.is_none() => text = Some(value),
            b"xmlUrl" => xml_url = Some(value),
            b"htmlUrl" => html_url = Some(value),
            _ => {} // 忽略未知属性（category/created/…）
        }
    }
    // `xmlUrl` 属性存在（含非法 URL）→ 订阅大纲；缺失 → 分组大纲。
    match xml_url {
        Some(raw) => {
            let url = safe_https_url(&raw);
            let title = text
                .filter(|value| !value.trim().is_empty())
                .map(|value| truncate_title(value.trim()))
                .unwrap_or_else(|| truncate_title(raw.trim()));
            outlines.push(OpmlOutline {
                title,
                xml_url: url,
                html_url: html_url.and_then(|url| safe_https_url(&url)),
                folder_path: folder_stack.join("/"),
            });
            if is_start {
                folder_stack.push(String::new()); // 占位：End 时弹出
            }
        }
        None => {
            // 分组大纲：压入段名（无标题压空串占位，保证 End 平衡）。
            if is_start {
                let segment = text
                    .filter(|value| !value.trim().is_empty())
                    .map(|value| truncate_title(value.trim()))
                    .unwrap_or_default();
                folder_stack.push(segment);
            }
        }
    }
    Ok(())
}

/// 导入订阅关系。`dry_run = true` 只计算计数不写库；执行模式在单个事务内
/// 完成。已存在源仅更新 `folder_path`/`title_override`（值变化才计入
/// `updated`），不触碰 `is_enabled`/`etag`/同步与阅读状态。
pub fn import_opml(conn: &Connection, xml: &str, dry_run: bool) -> AppResult<OpmlImportResult> {
    let outlines = parse_opml(xml.as_bytes())?;
    let mut result = OpmlImportResult::default();
    let mut seen: HashSet<String> = HashSet::new();

    let mut apply = |target: &Connection, result: &mut OpmlImportResult| -> AppResult<()> {
        for outline in &outlines {
            let Some(url) = outline.xml_url.as_deref() else {
                result.skipped += 1;
                continue;
            };
            if !seen.insert(url.to_string()) {
                result.skipped += 1; // 同一 OPML 内重复 URL：只保留首个
                continue;
            }
            match FeedRepository::get_source_by_feed_url(target, url)? {
                None => {
                    result.added += 1;
                    let id = uuid::Uuid::new_v4().to_string();
                    result.added_ids.push(id.clone());
                    if !dry_run {
                        FeedRepository::create_source(
                            target,
                            &NewFeedSource {
                                id,
                                feed_url: url.to_string(),
                                site_url: outline.html_url.clone(),
                                title: outline.title.clone(),
                                title_override: None,
                                description: None,
                                icon_url: None,
                                language: None,
                                folder_path: outline.folder_path.clone(),
                                fetch_interval_minutes: 60,
                            },
                            Utc::now(),
                        )?;
                    }
                }
                Some(existing) => {
                    // 显示标题一致性：override 与 OPML 相同，或未设置 override
                    // 且 feed 原标题与 OPML 相同，都视为无变化。
                    let override_matches = match &existing.title_override {
                        Some(override_title) => override_title == &outline.title,
                        None => existing.title == outline.title,
                    };
                    let changed = existing.folder_path != outline.folder_path || !override_matches;
                    if !changed {
                        result.skipped += 1;
                        continue;
                    }
                    result.updated += 1;
                    if !dry_run {
                        FeedRepository::update_source(
                            target,
                            &existing.id,
                            &FeedSourcePatch {
                                folder_path: Some(outline.folder_path.clone()),
                                title_override: Some(outline.title.clone()),
                                ..Default::default()
                            },
                            Utc::now(),
                        )?;
                    }
                }
            }
        }
        Ok(())
    };

    if dry_run {
        apply(conn, &mut result)?;
    } else if conn.is_autocommit() {
        let tx = conn.unchecked_transaction()?;
        apply(&tx, &mut result)?;
        tx.commit()?;
    } else {
        apply(conn, &mut result)?;
    }
    Ok(result)
}

/// 分组树：`children` 按段名字典序（`folder_path` 稳定排序），
/// `feeds` 为该节点路径下的订阅（`folder_path` 恰等于该节点路径）。
#[derive(Default)]
struct FolderNode {
    children: BTreeMap<String, FolderNode>,
    feeds: Vec<FeedSourceSummary>,
}

fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

/// 递归输出分组大纲；`feeds` 挂在所属节点，空分组（无订阅）不输出。
fn emit_folder(node: &FolderNode, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    for (segment, child) in &node.children {
        out.push_str(&format!(
            "{indent}<outline text=\"{}\">\n",
            xml_escape(segment)
        ));
        emit_folder(child, depth + 1, out);
        for feed in &child.feeds {
            emit_feed(feed, depth + 1, out);
        }
        out.push_str(&format!("{indent}</outline>\n"));
    }
}

fn emit_feed(feed: &FeedSourceSummary, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    let mut line = format!(
        "{indent}<outline type=\"rss\" text=\"{}\" xmlUrl=\"{}\"",
        xml_escape(&feed.title),
        xml_escape(&feed.feed_url),
    );
    if let Some(site_url) = &feed.site_url {
        line.push_str(&format!(" htmlUrl=\"{}\"", xml_escape(site_url)));
    }
    line.push_str("/>\n");
    out.push_str(&line);
}

/// 导出订阅关系：按 `folder_path` 稳定排序生成嵌套 OPML 2.0。
///
/// 导出只含 `text/xmlUrl/htmlUrl`（htmlUrl 可选）与分组 `text`，不含 ETag、
/// 错误、阅读状态、本地 ID 或时间戳；空 `folder_path` 的源直接挂在 `<body>`。
pub fn export_opml(conn: &Connection) -> AppResult<String> {
    let sources = FeedRepository::list_sources(conn)?;
    let mut root = FolderNode::default();
    for source in sources {
        let segments: Vec<&str> = source.folder_path.split('/').collect();
        let mut node = &mut root;
        for segment in segments.iter().filter(|segment| !segment.is_empty()) {
            node = node.children.entry((*segment).to_string()).or_default();
        }
        node.feeds.push(source);
    }

    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<opml version=\"2.0\">\n");
    out.push_str("  <head>\n    <title>Iris 订阅导出</title>\n  </head>\n");
    out.push_str("  <body>\n");
    for feed in &root.feeds {
        emit_feed(feed, 2, &mut out);
    }
    emit_folder(&root, 2, &mut out);
    out.push_str("  </body>\n</opml>\n");
    Ok(out)
}
