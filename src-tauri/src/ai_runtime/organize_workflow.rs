//! Organize workflow — generates suggestions for maintaining note library structure.
//!
//! This module implements the organize/audit workflow:
//! 1. Receive scope and task type
//! 2. Retrieve local notes list
//! 3. Analyze each note's title, tags, folder, links
//! 4. Generate organize suggestions (rule-based, no LLM dependency)
//! 5. Output batch change plan

use sha2::{Digest, Sha256};

use crate::ai_runtime::{
    OrganizeBatch, OrganizeSuggestion, OrganizeSuggestionType, OrganizeTaskInput,
    OrganizeTaskResult, OrganizeTaskType, TokenUsage,
};
use crate::error::AppResult;

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
    format!("org-{}", &hash[..12])
}

/// Generate a unique batch ID.
fn generate_batch_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut hasher = Sha256::new();
    hasher.update(timestamp.to_be_bytes());
    let hash = format!("{:x}", hasher.finalize());
    format!("batch-{}", &hash[..12])
}

/// Placeholder title patterns that indicate missing user-authored titles.
const PLACEHOLDER_TITLES: &[&str] = &["无标题", "新建文档", "untitled"];

/// Check if a title is a placeholder.
fn is_placeholder_title(title: &str) -> bool {
    let title_lower = title.to_lowercase();
    PLACEHOLDER_TITLES.iter().any(|p| title_lower.contains(p)) || title.trim().is_empty()
}

/// Check if a title is too long (over 100 characters).
fn is_title_too_long(title: &str) -> bool {
    title.len() > 100
}

/// Check if a title is too short (under 2 characters).
fn is_title_too_short(title: &str) -> bool {
    title.trim().len() < 2
}

/// Generate title suggestions for a file.
fn generate_title_suggestions(path: &str, title: &str, _content: &str) -> Vec<OrganizeSuggestion> {
    let mut suggestions = Vec::new();

    if is_placeholder_title(title) {
        suggestions.push(OrganizeSuggestion {
            id: generate_suggestion_id(),
            suggestion_type: OrganizeSuggestionType::RenameTitle,
            target_path: path.to_string(),
            current_value: Some(title.to_string()),
            suggested_value: "[需要用户输入有意义的标题]".to_string(),
            reason: "当前标题是占位符，建议输入有意义的标题".to_string(),
            source: "pattern_analysis".to_string(),
            confidence: 0.9,
            evidence_packet_ids: Vec::new(),
        });
    } else if is_title_too_long(title) {
        suggestions.push(OrganizeSuggestion {
            id: generate_suggestion_id(),
            suggestion_type: OrganizeSuggestionType::RenameTitle,
            target_path: path.to_string(),
            current_value: Some(title.to_string()),
            suggested_value: format!("{}...", &title[..50]),
            reason: "标题过长（超过100字符），建议精简".to_string(),
            source: "pattern_analysis".to_string(),
            confidence: 0.7,
            evidence_packet_ids: Vec::new(),
        });
    } else if is_title_too_short(title) {
        suggestions.push(OrganizeSuggestion {
            id: generate_suggestion_id(),
            suggestion_type: OrganizeSuggestionType::RenameTitle,
            target_path: path.to_string(),
            current_value: Some(title.to_string()),
            suggested_value: "[建议扩展标题以更准确描述内容]".to_string(),
            reason: "标题过短（少于2字符），建议扩展".to_string(),
            source: "pattern_analysis".to_string(),
            confidence: 0.6,
            evidence_packet_ids: Vec::new(),
        });
    }

    suggestions
}

/// Generate tag suggestions for a file.
fn generate_tag_suggestions(
    path: &str,
    title: &str,
    tags: &[String],
    content: &str,
) -> Vec<OrganizeSuggestion> {
    let mut suggestions = Vec::new();

    // Check if note has no tags
    if tags.is_empty() {
        // Extract potential tags from content
        let suggested_tags = extract_tags_from_content(content);
        if !suggested_tags.is_empty() {
            suggestions.push(OrganizeSuggestion {
                id: generate_suggestion_id(),
                suggestion_type: OrganizeSuggestionType::AddTag,
                target_path: path.to_string(),
                current_value: None,
                suggested_value: suggested_tags.join(", "),
                reason: "笔记缺少标签，根据内容提取了建议标签".to_string(),
                source: "content_analysis".to_string(),
                confidence: 0.7,
                evidence_packet_ids: Vec::new(),
            });
        }
    }

    // Check if title suggests a tag but not present
    let title_tag = extract_tag_from_title(title);
    if let Some(tag) = title_tag {
        if !tags.contains(&tag) {
            suggestions.push(OrganizeSuggestion {
                id: generate_suggestion_id(),
                suggestion_type: OrganizeSuggestionType::AddTag,
                target_path: path.to_string(),
                current_value: None,
                suggested_value: tag.clone(),
                reason: format!("标题暗示了标签「{}」但未添加", tag),
                source: "title_analysis".to_string(),
                confidence: 0.6,
                evidence_packet_ids: Vec::new(),
            });
        }
    }

    suggestions
}

/// Extract potential tags from content.
fn extract_tags_from_content(content: &str) -> Vec<String> {
    let mut tags = Vec::new();

    // Extract hashtags from content
    let re = regex::Regex::new(r"#[\w\u{4e00}-\u{9fff}\-]+").unwrap();
    for cap in re.find_iter(content) {
        let tag = cap.as_str().trim_start_matches('#').to_string();
        if !tag.is_empty() && tag.len() <= 64 && !tags.contains(&tag) {
            tags.push(tag);
        }
    }

    // Extract tags from bold text patterns
    let re = regex::Regex::new(r"\*\*([^*]+)\*\*").unwrap();
    for cap in re.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            let text = m.as_str().trim();
            if text.len() >= 2 && text.len() <= 20 && !tags.contains(&text.to_string()) {
                tags.push(text.to_string());
            }
        }
    }

    tags.truncate(5); // Limit to 5 suggestions
    tags
}

/// Extract a tag from the title.
fn extract_tag_from_title(title: &str) -> Option<String> {
    // If title contains common patterns like "XXX笔记", "XXX记录"
    let patterns = ["笔记", "记录", "总结", "方案", "报告"];
    for pattern in &patterns {
        if title.contains(pattern) {
            let idx = title.find(pattern).unwrap_or(0);
            if idx > 0 {
                return Some(title[..idx].to_string());
            }
        }
    }
    None
}

/// Generate folder suggestions for a file.
fn generate_folder_suggestions(path: &str, _title: &str) -> Vec<OrganizeSuggestion> {
    let mut suggestions = Vec::new();

    // Check if file is in root directory
    if !path.contains('/') {
        suggestions.push(OrganizeSuggestion {
            id: generate_suggestion_id(),
            suggestion_type: OrganizeSuggestionType::MoveToFolder,
            target_path: path.to_string(),
            current_value: Some("/".to_string()),
            suggested_value: "[建议的文件夹]/".to_string(),
            reason: "笔记位于根目录，建议归类到合适的文件夹".to_string(),
            source: "structure_analysis".to_string(),
            confidence: 0.5,
            evidence_packet_ids: Vec::new(),
        });
    }

    suggestions
}

/// Generate corpus assignment suggestions for a file.
fn generate_corpus_suggestions(
    path: &str,
    title: &str,
    tags: &[String],
) -> Vec<OrganizeSuggestion> {
    let mut suggestions = Vec::new();

    // Check if file matches a corpus pattern
    let corpus_patterns = [
        ("regulation", vec!["法规", "条例", "规定", "办法"]),
        ("exemplar", vec!["范文", "示例", "样例", "模板"]),
        ("meeting", vec!["会议", "纪要", "记录"]),
    ];

    for (corpus_id, keywords) in &corpus_patterns {
        let matches = keywords
            .iter()
            .any(|kw| title.contains(kw) || tags.iter().any(|t| t.contains(kw)));
        if matches {
            suggestions.push(OrganizeSuggestion {
                id: generate_suggestion_id(),
                suggestion_type: OrganizeSuggestionType::AssignCorpus,
                target_path: path.to_string(),
                current_value: None,
                suggested_value: corpus_id.to_string(),
                reason: format!("根据标题或标签，建议归入「{}」语料库", corpus_id),
                source: "keyword_analysis".to_string(),
                confidence: 0.6,
                evidence_packet_ids: Vec::new(),
            });
        }
    }

    suggestions
}

/// File metadata for organize analysis.
#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub path: String,
    pub title: String,
    pub tags: Vec<String>,
    pub content_hash: String,
    pub word_count: i64,
}

/// Execute organize task with file metadata.
pub fn execute_organize_with_metadata(
    input: &OrganizeTaskInput,
    files: Vec<FileMetadata>,
) -> AppResult<OrganizeTaskResult> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut all_suggestions = Vec::new();

    for file in &files {
        match input.task_type {
            OrganizeTaskType::FullAudit => {
                all_suggestions.extend(generate_title_suggestions(&file.path, &file.title, ""));
                all_suggestions.extend(generate_tag_suggestions(
                    &file.path,
                    &file.title,
                    &file.tags,
                    "",
                ));
                all_suggestions.extend(generate_folder_suggestions(&file.path, &file.title));
                all_suggestions.extend(generate_corpus_suggestions(
                    &file.path,
                    &file.title,
                    &file.tags,
                ));
            }
            OrganizeTaskType::TitleSuggestions => {
                all_suggestions.extend(generate_title_suggestions(&file.path, &file.title, ""));
            }
            OrganizeTaskType::TagSuggestions => {
                all_suggestions.extend(generate_tag_suggestions(
                    &file.path,
                    &file.title,
                    &file.tags,
                    "",
                ));
            }
            OrganizeTaskType::FolderSuggestions => {
                all_suggestions.extend(generate_folder_suggestions(&file.path, &file.title));
            }
            OrganizeTaskType::LinkSuggestions => {
                // Link suggestions are handled separately via graph module
            }
        }
    }

    // Generate batch
    let batch = OrganizeBatch {
        id: generate_batch_id(),
        title: match input.task_type {
            OrganizeTaskType::FullAudit => "笔记库全面审计".to_string(),
            OrganizeTaskType::TitleSuggestions => "标题优化建议".to_string(),
            OrganizeTaskType::TagSuggestions => "标签添加建议".to_string(),
            OrganizeTaskType::FolderSuggestions => "文件夹归类建议".to_string(),
            OrganizeTaskType::LinkSuggestions => "链接建议".to_string(),
        },
        description: format!(
            "共分析 {} 篇笔记，生成 {} 条建议",
            files.len(),
            all_suggestions.len()
        ),
        suggestions: all_suggestions,
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
    };

    Ok(OrganizeTaskResult {
        request_id,
        batch,
        total_tokens: TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_placeholder_title() {
        assert!(is_placeholder_title("无标题1"));
        assert!(is_placeholder_title("新建文档"));
        assert!(is_placeholder_title("untitled-123"));
        assert!(is_placeholder_title(""));
        assert!(!is_placeholder_title("我的笔记"));
        assert!(!is_placeholder_title("SQLite 入门"));
    }

    #[test]
    fn test_is_title_too_long() {
        assert!(!is_title_too_long("短标题"));
        assert!(is_title_too_long(&"a".repeat(101)));
        assert!(!is_title_too_long(&"a".repeat(100)));
    }

    #[test]
    fn test_is_title_too_short() {
        assert!(is_title_too_short("a"));
        assert!(is_title_too_short(""));
        assert!(!is_title_too_short("正常标题"));
    }

    #[test]
    fn test_extract_tags_from_content() {
        let content = "这是 #rust 和 #tauri 的笔记";
        let tags = extract_tags_from_content(content);
        assert!(tags.contains(&"rust".to_string()));
        assert!(tags.contains(&"tauri".to_string()));
    }

    #[test]
    fn test_extract_tag_from_title() {
        assert_eq!(
            extract_tag_from_title("SQLite笔记"),
            Some("SQLite".to_string())
        );
        assert_eq!(extract_tag_from_title("笔记"), None);
        assert_eq!(extract_tag_from_title("项目总结"), Some("项目".to_string()));
    }

    #[test]
    fn test_generate_title_suggestions_placeholder() {
        let suggestions = generate_title_suggestions("test.md", "无标题1", "");
        assert_eq!(suggestions.len(), 1);
        assert_eq!(
            suggestions[0].suggestion_type,
            OrganizeSuggestionType::RenameTitle
        );
    }

    #[test]
    fn test_generate_title_suggestions_normal() {
        let suggestions = generate_title_suggestions("test.md", "正常标题", "");
        assert_eq!(suggestions.len(), 0);
    }

    #[test]
    fn test_generate_tag_suggestions_empty() {
        let suggestions = generate_tag_suggestions("test.md", "标题", &[], "内容 #tag1 #tag2");
        assert!(!suggestions.is_empty());
    }

    #[test]
    fn test_generate_folder_suggestions_root() {
        let suggestions = generate_folder_suggestions("test.md", "标题");
        assert_eq!(suggestions.len(), 1);
    }

    #[test]
    fn test_generate_folder_suggestions_nested() {
        let suggestions = generate_folder_suggestions("folder/test.md", "标题");
        assert_eq!(suggestions.len(), 0);
    }
}
