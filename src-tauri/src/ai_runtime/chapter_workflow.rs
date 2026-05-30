//! Chapter-level writing workflow — handles chapter rewrite, continue, restructure.
//!
//! This module extends the writing workflow to support chapter-level operations:
//! 1. Parse document structure (heading hierarchy)
//! 2. Identify chapter boundaries
//! 3. Generate chapter-level writing suggestions
//! 4. Generate chunked patches (multiple PatchProposals)

use sha2::{Digest, Sha256};
use tauri::AppHandle;

use crate::ai_runtime::model_gateway::{GatewayRequest, LlmMessage, MessageRole, ProviderConfig};
use crate::ai_runtime::{
    ContextPacket, PatchProposal, RiskLevel, SourceSpan, TokenUsage, WritingIntent,
    WritingSuggestion,
};
use crate::error::AppResult;
use crate::storage::db::Database;
/// Chapter information extracted from document structure.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChapterInfo {
    /// Heading level (1-6)
    pub heading_level: usize,
    /// Heading text (without # markers)
    pub heading_text: String,
    /// Content start offset (character position after heading line)
    pub content_start: usize,
    /// Content end offset (before next heading or end of document)
    pub content_end: usize,
    /// Chapter content (including heading)
    pub content: String,
    /// Full heading path (e.g., "第一章 > 第一条")
    pub heading_path: String,
}

/// Chapter-level writing task input.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChapterWritingInput {
    /// Target file relative path
    pub target_path: String,
    /// Base content hash (SHA-256)
    pub base_content_hash: String,
    /// Chapter information
    pub chapter: ChapterInfo,
    /// Writing goal
    pub writing_goal: String,
    /// Whether web search is authorized
    pub web_authorized: bool,
}

/// Chapter-level writing task result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChapterWritingResult {
    /// Request ID
    pub request_id: String,
    /// Writing suggestions
    pub suggestions: Vec<WritingSuggestion>,
    /// Patches (may be multiple for chapter-level operations)
    pub patches: Vec<PatchProposal>,
    /// Evidence used
    pub evidence_used: Vec<ContextPacket>,
    /// Token usage
    pub total_tokens: TokenUsage,
}

/// Generate a unique suggestion ID.
fn generate_suggestion_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut hasher = Sha256::new();
    hasher.update(timestamp.to_be_bytes());
    let hash = format!("{:x}", hasher.finalize());
    format!("sug-{}", &hash[..12])
}

/// Parse document structure and extract chapters.
pub fn parse_chapters(content: &str) -> Vec<ChapterInfo> {
    let mut chapters = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    let mut current_offset = 0;

    let mut current_chapter: Option<(usize, String, usize)> = None;

    for (i, line) in lines.iter().enumerate() {
        let is_last = i == total_lines - 1;
        let line_len = if is_last && !content.ends_with('\n') {
            line.len()
        } else {
            line.len() + 1 // +1 for newline
        };

        if let Some(level) = heading_level(line) {
            // Save previous chapter if exists
            if let Some((prev_level, prev_text, prev_start)) = current_chapter.take() {
                let heading_path = build_heading_path(&chapters, prev_level, &prev_text);
                let end = current_offset.min(content.len());
                chapters.push(ChapterInfo {
                    heading_level: prev_level,
                    heading_text: prev_text,
                    content_start: prev_start,
                    content_end: end,
                    content: content[prev_start..end].to_string(),
                    heading_path,
                });
            }

            // Start new chapter
            let heading_text = line.trim_start_matches('#').trim().to_string();
            current_chapter = Some((level, heading_text, current_offset));
        }

        current_offset += line_len;
    }

    // Save last chapter
    if let Some((level, text, start)) = current_chapter {
        let heading_path = build_heading_path(&chapters, level, &text);
        let end = content.len();
        chapters.push(ChapterInfo {
            heading_level: level,
            heading_text: text,
            content_start: start,
            content_end: end,
            content: content[start..end].to_string(),
            heading_path,
        });
    }

    // If no chapters found, treat entire content as one chapter
    if chapters.is_empty() {
        chapters.push(ChapterInfo {
            heading_level: 0,
            heading_text: "(文档全文)".to_string(),
            content_start: 0,
            content_end: content.len(),
            content: content.to_string(),
            heading_path: "(文档全文)".to_string(),
        });
    }

    chapters
}

/// Get heading level from a line (count # markers).
fn heading_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }

    let level = trimmed.chars().take_while(|&c| c == '#').count();
    if level > 0 && level <= 6 && (trimmed.chars().nth(level) == Some(' ')) {
        Some(level)
    } else {
        None
    }
}

/// Build heading path for display.
fn build_heading_path(chapters: &[ChapterInfo], level: usize, text: &str) -> String {
    let mut path_parts = Vec::new();

    // Find parent headings
    for ch in chapters.iter().rev() {
        if ch.heading_level < level {
            path_parts.insert(0, ch.heading_text.clone());
            if ch.heading_level == 1 {
                break;
            }
        }
    }

    path_parts.push(text.to_string());
    path_parts.join(" > ")
}

/// Detect chapter-level writing intent from goal.
pub fn detect_chapter_intent(goal: &str) -> WritingIntent {
    let goal_lower = goal.to_lowercase();

    if goal_lower.contains("重排")
        || goal_lower.contains("调整结构")
        || goal_lower.contains("重组")
        || goal_lower.contains("restructure")
    {
        WritingIntent::ChapterRestructure
    } else if goal_lower.contains("续写")
        || goal_lower.contains("继续")
        || goal_lower.contains("接着写")
        || goal_lower.contains("continue")
    {
        WritingIntent::ChapterContinue
    } else {
        // Default to chapter rewrite
        WritingIntent::ChapterRewrite
    }
}

/// Build chapter-level writing suggestion.
pub fn build_chapter_suggestion(
    intent: WritingIntent,
    chapter: &ChapterInfo,
    explanation: &str,
    confidence: f64,
) -> WritingSuggestion {
    WritingSuggestion {
        id: generate_suggestion_id(),
        intent,
        explanation: format!("[{}] {}", chapter.heading_text, explanation),
        confidence,
    }
}

/// Compute SHA-256 hash of content.
pub fn compute_content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Build a patch proposal for a chapter.
pub fn build_chapter_patch(
    target_path: &str,
    base_content_hash: &str,
    chapter: &ChapterInfo,
    replacement: &str,
    evidence_ids: Vec<String>,
) -> PatchProposal {
    let original = &chapter.content;
    let risk_level = assess_chapter_risk(original, replacement);
    let warnings = generate_chapter_warnings(original, replacement, risk_level);

    PatchProposal {
        id: crate::ai_runtime::writing_workflow::generate_patch_id(),
        target_path: target_path.to_string(),
        base_content_hash: base_content_hash.to_string(),
        range: SourceSpan {
            start: chapter.content_start,
            end: chapter.content_end,
        },
        original_text: original.to_string(),
        replacement_text: replacement.to_string(),
        evidence_packet_ids: evidence_ids,
        risk_level,
        warnings,
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
    }
}

/// Assess risk level for chapter-level patch.
fn assess_chapter_risk(original: &str, replacement: &str) -> RiskLevel {
    let original_len = original.len();
    let replacement_len = replacement.len();
    let size_ratio = if original_len > 0 {
        replacement_len as f64 / original_len as f64
    } else {
        replacement_len as f64
    };

    // Chapter-level patches are inherently higher risk
    if replacement_len > 500 || !(0.3..=3.0).contains(&size_ratio) {
        RiskLevel::High
    } else {
        RiskLevel::Medium
    }
}

/// Generate warnings for chapter-level patch.
fn generate_chapter_warnings(
    original: &str,
    replacement: &str,
    risk_level: RiskLevel,
) -> Vec<String> {
    let mut warnings = Vec::new();

    warnings.push("章节级修改：将影响整个章节内容".to_string());

    if risk_level == RiskLevel::High {
        warnings.push("高风险：章节内容变化较大，请仔细检查".to_string());
    }

    let original_lines = original.lines().count();
    let replacement_lines = replacement.lines().count();
    let line_diff = (replacement_lines as i64 - original_lines as i64).abs();

    if line_diff > 10 {
        warnings.push(format!(
            "行数变化较大：{original_lines} → {replacement_lines}"
        ));
    }

    // Check for heading changes
    let original_headings = original.lines().filter(|l| l.starts_with('#')).count();
    let replacement_headings = replacement.lines().filter(|l| l.starts_with('#')).count();
    if replacement_headings != original_headings {
        warnings.push("章节标题结构发生变化".to_string());
    }

    warnings
}

fn chapter_intent_instruction(intent: &WritingIntent) -> &'static str {
    match intent {
        WritingIntent::ChapterContinue => {
            "续写该章节：保留标题结构，在现有内容后自然延伸，不重复已有论述。"
        }
        WritingIntent::ChapterRestructure => {
            "重排该章节结构与段落顺序，保留核心信息，优化层次与衔接。"
        }
        _ => "改写该章节：改进结构、表达与论证，保留章节标题行（# 级别不变）。",
    }
}

/// 调用 LLM 生成章节级替换 Markdown（含章节标题行）。
#[allow(clippy::too_many_arguments)]
pub async fn generate_chapter_content_with_llm(
    db: &Database,
    app_handle: &AppHandle,
    provider: &ProviderConfig,
    intent: &WritingIntent,
    chapter: &ChapterInfo,
    full_document: &str,
    goal: &str,
    evidence: &[ContextPacket],
) -> AppResult<(String, TokenUsage)> {
    let rules = crate::ai_runtime::model_gateway::ModelGateway::load_active_rules_for_scene(
        db,
        crate::ai_runtime::AiScene::DraftingAssist,
    )?;
    let system = crate::ai_runtime::model_gateway::ModelGateway::build_system_prompt(
        crate::ai_runtime::AiScene::DraftingAssist,
        evidence,
        &rules,
        false,
    );

    let evidence_block = evidence
        .iter()
        .take(12)
        .map(|p| format!("{} {} — {}", p.citation_label, p.title, p.excerpt))
        .collect::<Vec<_>>()
        .join("\n");

    let user = format!(
        "{}\n\n写作目标：{goal}\n\n章节路径：{}\n\n整篇文档（供语境参考，勿全文照抄）：\n{}\n\n待处理章节原文：\n{}\n\n可用证据：\n{evidence_block}\n\n请只输出替换后的完整章节 Markdown（从章节标题行开始到章节结束），不要输出解释或代码围栏。",
        chapter_intent_instruction(intent),
        chapter.heading_path,
        if full_document.len() > 4000 {
            format!("{}…", &full_document[..4000])
        } else {
            full_document.to_string()
        },
        chapter.content,
    );

    let request = GatewayRequest {
        provider: provider.clone(),
        messages: vec![
            LlmMessage {
                role: MessageRole::System,
                content: system,
                tool_call_id: None,
                tool_calls: None,
            },
            LlmMessage {
                role: MessageRole::User,
                content: user,
                tool_call_id: None,
                tool_calls: None,
            },
        ],
        tools: vec![],
        max_tokens: Some(4096),
        temperature: Some(0.4),
        stream: false,
    };

    let gateway = crate::ai_runtime::model_gateway::ModelGateway::with_defaults(
        app_handle.clone(),
        vec![provider.clone()],
    )?;
    let response = gateway.send_request(request).await?;
    let usage = response.usage;
    let mut text = response.content.unwrap_or_default().trim().to_string();
    if text.starts_with("```") {
        let inner = text
            .trim_start_matches('`')
            .trim_start_matches(|c: char| c.is_alphanumeric() || c == '\n')
            .trim();
        text = inner.trim_end_matches('`').trim().to_string();
    }
    if text.is_empty() {
        text = chapter.content.clone();
    }
    Ok((
        text,
        TokenUsage {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            ..Default::default()
        },
    ))
}

/// 章节 LLM 不可用时的结构化回退（保留标题行，按意图做最小改写）。
pub fn chapter_heuristic_fallback(
    intent: &WritingIntent,
    chapter: &ChapterInfo,
    goal: &str,
) -> String {
    match intent {
        WritingIntent::ChapterContinue => {
            let mut out = chapter.content.trim_end().to_string();
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&format!(
                "\n\n> **续写提示**（{goal}）：请在此补充后续段落，并与上文语气一致。\n"
            ));
            out
        }
        WritingIntent::ChapterRestructure => restructure_chapter_paragraphs(&chapter.content),
        _ => normalize_chapter_markdown(&chapter.content),
    }
}

fn normalize_chapter_markdown(content: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut saw_heading = false;
    for line in content.lines() {
        let trimmed = line.trim_end();
        if trimmed.starts_with('#') {
            if saw_heading && !lines.last().is_some_and(|l| l.is_empty()) {
                lines.push(String::new());
            }
            saw_heading = true;
            lines.push(trimmed.to_string());
        } else if trimmed.is_empty() {
            if !lines.last().is_some_and(|l| l.is_empty()) {
                lines.push(String::new());
            }
        } else {
            lines.push(trimmed.to_string());
        }
    }
    lines.join("\n")
}

fn restructure_chapter_paragraphs(content: &str) -> String {
    let mut heading_lines = Vec::new();
    let mut body_paragraphs: Vec<String> = Vec::new();
    let mut current = String::new();

    for line in content.lines() {
        if line.trim_start().starts_with('#') {
            if !current.trim().is_empty() {
                body_paragraphs.push(current.trim().to_string());
                current.clear();
            }
            heading_lines.push(line.trim_end().to_string());
        } else if line.trim().is_empty() {
            if !current.trim().is_empty() {
                body_paragraphs.push(current.trim().to_string());
                current.clear();
            }
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(line.trim());
        }
    }
    if !current.trim().is_empty() {
        body_paragraphs.push(current.trim().to_string());
    }

    body_paragraphs.sort_by_key(|p| std::cmp::Reverse(p.len()));

    let mut out = heading_lines.join("\n");
    if !out.is_empty() && !body_paragraphs.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(&body_paragraphs.join("\n\n"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chapter_heuristic_continue() {
        let chapter = ChapterInfo {
            heading_level: 2,
            heading_text: "小节".to_string(),
            content_start: 0,
            content_end: 20,
            content: "## 小节\n\n已有内容。".to_string(),
            heading_path: "文档 > 小节".to_string(),
        };
        let out = chapter_heuristic_fallback(&WritingIntent::ChapterContinue, &chapter, "续写论证");
        assert!(out.contains("续写提示"));
        assert!(out.contains("已有内容"));
    }

    #[test]
    fn test_parse_chapters_empty() {
        let content = "Hello, World!";
        let chapters = parse_chapters(content);
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].heading_text, "(文档全文)");
    }

    #[test]
    fn test_parse_chapters_with_headings() {
        let content = "# Chapter 1\n\nSome content\n\n## Section 1.1\n\nMore content\n\n# Chapter 2\n\nFinal content";
        let chapters = parse_chapters(content);
        assert_eq!(chapters.len(), 3);
        assert_eq!(chapters[0].heading_text, "Chapter 1");
        assert_eq!(chapters[0].heading_level, 1);
        assert_eq!(chapters[1].heading_text, "Section 1.1");
        assert_eq!(chapters[1].heading_level, 2);
        assert_eq!(chapters[2].heading_text, "Chapter 2");
    }

    #[test]
    fn test_heading_level() {
        assert_eq!(heading_level("# Heading"), Some(1));
        assert_eq!(heading_level("## Heading"), Some(2));
        assert_eq!(heading_level("### Heading"), Some(3));
        assert_eq!(heading_level("Not a heading"), None);
        assert_eq!(heading_level("#NoSpace"), None);
    }

    #[test]
    fn test_detect_chapter_intent() {
        assert!(matches!(
            detect_chapter_intent("重排这个章节"),
            WritingIntent::ChapterRestructure
        ));
        assert!(matches!(
            detect_chapter_intent("续写本章"),
            WritingIntent::ChapterContinue
        ));
        assert!(matches!(
            detect_chapter_intent("改写这一章"),
            WritingIntent::ChapterRewrite
        ));
    }

    #[test]
    fn test_assess_chapter_risk() {
        // Small change -> Medium (minimum for chapter)
        assert_eq!(
            assess_chapter_risk("short text", "another short text"),
            RiskLevel::Medium
        );
        // Large change -> High
        assert_eq!(assess_chapter_risk("a", &"b".repeat(3000)), RiskLevel::High);
    }
}
