//! Document-level check workflow — handles outline check, citation gap check, style consistency.
//!
//! This module implements document-level analysis:
//! 1. Parse document structure (outline, chapters)
//! 2. Check outline completeness and consistency
//! 3. Check citation gaps (uncited claims)
//! 4. Check style consistency across document
//! 5. Cross-document reference suggestion

use sha2::{Digest, Sha256};

use crate::ai_runtime::{
    CitationAction, CitationSuggestion, ContextPacket, FactClaim, PatchProposal, TokenUsage,
};
use crate::error::AppResult;

/// Document check type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentCheckType {
    /// Outline check
    OutlineCheck,
    /// Citation gap check
    CitationGapCheck,
    /// Style consistency check
    StyleConsistency,
    /// Cross-document reference suggestion
    CrossDocReference,
}

/// Document check input.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocumentCheckInput {
    /// Target file relative path
    pub target_path: String,
    /// Document content
    pub content: String,
    /// Check type
    pub check_type: DocumentCheckType,
    /// Whether web search is authorized
    pub web_authorized: bool,
}

/// Document check result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocumentCheckResult {
    /// Request ID
    pub request_id: String,
    /// Check type
    pub check_type: DocumentCheckType,
    /// Outline check result (if applicable)
    pub outline_result: Option<OutlineCheckResult>,
    /// Citation gap check result (if applicable)
    pub citation_gap_result: Option<CitationGapCheckResult>,
    /// Style consistency check result (if applicable)
    pub style_result: Option<StyleConsistencyResult>,
    /// Suggested patches
    pub patches: Vec<PatchProposal>,
    /// Evidence used
    pub evidence_used: Vec<ContextPacket>,
    /// Token usage
    pub total_tokens: TokenUsage,
}

/// Outline check result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OutlineCheckResult {
    /// Issues found
    pub issues: Vec<OutlineIssue>,
    /// Suggestions for improvement
    pub suggestions: Vec<OutlineSuggestion>,
    /// Outline entries
    pub outline_entries: Vec<OutlineEntry>,
}

/// Outline entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OutlineEntry {
    /// Heading level (1-6)
    pub level: usize,
    /// Heading text
    pub text: String,
    /// Position in document (character offset)
    pub position: usize,
    /// Word count under this heading
    pub word_count: usize,
}

/// Outline issue type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutlineIssueType {
    /// Missing heading (large content block without heading)
    MissingHeading,
    /// Skipped heading level (e.g., H1 -> H3)
    SkippedLevel,
    /// Empty heading (heading with no content)
    EmptyHeading,
    /// Too deep nesting (excessive heading depth)
    TooDeepNesting,
    /// Inconsistent heading style
    InconsistentStyle,
}

/// Issue severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    /// Low severity
    Low,
    /// Medium severity
    Medium,
    /// High severity
    High,
}

/// Outline issue.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OutlineIssue {
    /// Issue type
    pub issue_type: OutlineIssueType,
    /// Heading path where issue occurs
    pub heading_path: String,
    /// Description of the issue
    pub description: String,
    /// Severity
    pub severity: IssueSeverity,
    /// Position in document
    pub position: usize,
}

/// Outline suggestion.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OutlineSuggestion {
    /// Suggestion text
    pub suggestion: String,
    /// Position to apply suggestion
    pub position: usize,
    /// Whether this requires a patch
    pub requires_patch: bool,
}

/// Citation gap check result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CitationGapCheckResult {
    /// Uncited claims
    pub uncited_claims: Vec<FactClaim>,
    /// Weak citations
    pub weak_citations: Vec<WeakCitation>,
    /// Suggestions
    pub suggestions: Vec<CitationSuggestion>,
}

/// Weak citation info.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WeakCitation {
    /// Claim text
    pub claim: String,
    /// Current citation
    pub current_citation: String,
    /// Why it's weak
    pub reason: String,
    /// Suggested stronger citation
    pub suggested_citation: Option<String>,
}

/// Style consistency check result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StyleConsistencyResult {
    /// Inconsistencies found
    pub inconsistencies: Vec<StyleInconsistency>,
    /// Suggestions
    pub suggestions: Vec<StyleSuggestion>,
    /// Overall style score (0.0 - 1.0)
    pub consistency_score: f64,
}

/// Style inconsistency.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StyleInconsistency {
    /// Inconsistency type
    pub inconsistency_type: StyleInconsistencyType,
    /// Location
    pub location: String,
    /// Description
    pub description: String,
    /// Examples of inconsistency
    pub examples: Vec<String>,
}

/// Style inconsistency type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StyleInconsistencyType {
    /// Mixed formal/informal tone
    ToneMismatch,
    /// Inconsistent terminology
    TerminologyMismatch,
    /// Mixed citation formats
    CitationFormatMismatch,
    /// Inconsistent list formatting
    ListFormatMismatch,
    /// Mixed date/number formats
    DateFormatMismatch,
    /// Inconsistent punctuation style
    PunctuationMismatch,
}

/// Style suggestion.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StyleSuggestion {
    /// Suggestion text
    pub suggestion: String,
    /// Locations to apply
    pub locations: Vec<String>,
    /// Whether this requires a patch
    pub requires_patch: bool,
}

/// Generate a unique request ID.
fn generate_request_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut hasher = Sha256::new();
    hasher.update(timestamp.to_be_bytes());
    let hash = format!("{:x}", hasher.finalize());
    format!("doc-{}", &hash[..12])
}

/// Parse outline from content.
pub fn parse_outline(content: &str) -> Vec<OutlineEntry> {
    let mut entries = Vec::new();
    let mut offset = 0;

    for line in content.lines() {
        let line_len = line.len() + 1; // +1 for newline

        if let Some(level) = heading_level(line) {
            let text = line.trim_start_matches('#').trim().to_string();
            if !text.is_empty() {
                // Count words under this heading (until next heading)
                let remaining = &content[offset + line_len..];
                let word_count = count_words_until_next_heading(remaining);

                entries.push(OutlineEntry {
                    level,
                    text,
                    position: offset,
                    word_count,
                });
            }
        }

        offset += line_len;
    }

    entries
}

/// Get heading level from a line.
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

/// Count words until next heading.
fn count_words_until_next_heading(content: &str) -> usize {
    let mut word_count = 0;
    for line in content.lines() {
        if heading_level(line).is_some() {
            break;
        }
        word_count += line.split_whitespace().count();
    }
    word_count
}

/// Check outline for issues.
pub fn check_outline(entries: &[OutlineEntry]) -> OutlineCheckResult {
    let mut issues = Vec::new();
    let mut suggestions = Vec::new();

    if entries.is_empty() {
        return OutlineCheckResult {
            issues,
            suggestions,
            outline_entries: entries.to_vec(),
        };
    }

    // Check for skipped heading levels
    for i in 1..entries.len() {
        let prev = &entries[i - 1];
        let curr = &entries[i];

        if curr.level > prev.level + 1 {
            issues.push(OutlineIssue {
                issue_type: OutlineIssueType::SkippedLevel,
                heading_path: curr.text.clone(),
                description: format!(
                    "标题级别跳跃：从 H{} 直接到 H{}，建议添加中间级别",
                    prev.level, curr.level
                ),
                severity: IssueSeverity::Medium,
                position: curr.position,
            });
        }
    }

    // Check for empty headings (headings with very little content)
    for entry in entries {
        if entry.word_count < 5 && entry.level < 4 {
            issues.push(OutlineIssue {
                issue_type: OutlineIssueType::EmptyHeading,
                heading_path: entry.text.clone(),
                description: format!(
                    "标题「{}」下内容过少（{}字），建议补充内容或合并到上级",
                    entry.text, entry.word_count
                ),
                severity: IssueSeverity::Low,
                position: entry.position,
            });
        }
    }

    // Check for too deep nesting
    for entry in entries {
        if entry.level > 4 {
            issues.push(OutlineIssue {
                issue_type: OutlineIssueType::TooDeepNesting,
                heading_path: entry.text.clone(),
                description: format!(
                    "标题层级过深（H{}），建议简化结构或合并到上级标题",
                    entry.level
                ),
                severity: IssueSeverity::Low,
                position: entry.position,
            });
        }
    }

    // Check for large content blocks without headings
    for entry in entries {
        if entry.word_count > 500 {
            suggestions.push(OutlineSuggestion {
                suggestion: format!(
                    "标题「{}」下有 {} 字内容，建议拆分为子章节",
                    entry.text, entry.word_count
                ),
                position: entry.position,
                requires_patch: false,
            });
        }
    }

    OutlineCheckResult {
        issues,
        suggestions,
        outline_entries: entries.to_vec(),
    }
}

/// Extract claims from content for citation gap analysis.
fn extract_claims_from_content(content: &str) -> Vec<FactClaim> {
    let mut claims = Vec::new();

    // Split by sentence-ending punctuation
    let sentences: Vec<&str> = content
        .split(['。', '！', '？', '.', '!', '?'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && s.len() > 10)
        .collect();

    for sentence in sentences {
        // Check if sentence looks like a factual claim
        if is_likely_claim(sentence) {
            claims.push(FactClaim {
                id: generate_claim_id(),
                statement: sentence.to_string(),
                has_support: false,
                supporting_evidence: Vec::new(),
                conflicting_evidence: Vec::new(),
            });
        }
    }

    claims
}

/// Generate a unique claim ID.
fn generate_claim_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut hasher = Sha256::new();
    hasher.update(timestamp.to_be_bytes());
    let hash = format!("{:x}", hasher.finalize());
    format!("claim-{}", &hash[..12])
}

/// Check if a sentence is likely a factual claim.
fn is_likely_claim(text: &str) -> bool {
    let text_lower = text.to_lowercase();

    let has_numbers = text.chars().any(|c| c.is_ascii_digit());
    let has_citation_markers = text_lower.contains("根据")
        || text_lower.contains("按照")
        || text_lower.contains("依据")
        || text_lower.contains("according")
        || text_lower.contains("based on");
    let has_factual_verbs = text_lower.contains("是")
        || text_lower.contains("有")
        || text_lower.contains("为")
        || text_lower.contains("was")
        || text_lower.contains("is")
        || text_lower.contains("are");
    let is_long_enough = text.len() >= 10;

    let criteria_count = [
        has_numbers,
        has_citation_markers,
        has_factual_verbs,
        is_long_enough,
    ]
    .iter()
    .filter(|&&x| x)
    .count();

    criteria_count >= 2
}

/// Analyze style consistency.
fn analyze_style_consistency(content: &str) -> StyleConsistencyResult {
    let mut inconsistencies = Vec::new();
    let mut suggestions = Vec::new();

    // Check for mixed formal/informal tone
    let formal_markers = ["您", "请", "敬请", "恳请"];
    let informal_markers = ["你", "咱们", "咱们"];

    let has_formal = formal_markers.iter().any(|m| content.contains(m));
    let has_informal = informal_markers.iter().any(|m| content.contains(m));

    if has_formal && has_informal {
        inconsistencies.push(StyleInconsistency {
            inconsistency_type: StyleInconsistencyType::ToneMismatch,
            location: "全文".to_string(),
            description: "文档中混用了正式和非正式语气".to_string(),
            examples: vec!["您".to_string(), "你".to_string()],
        });
    }

    // Check for inconsistent list formatting
    let has_dash_list = content.contains("\n- ");
    let has_star_list = content.contains("\n* ");
    let has_number_list = content.lines().any(|l| {
        let trimmed = l.trim_start();
        trimmed.len() > 2
            && trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
            && trimmed
                .chars()
                .nth(1)
                .is_some_and(|c| c == '.' || c == ')')
    });

    let list_formats = [has_dash_list, has_star_list, has_number_list]
        .iter()
        .filter(|&&x| x)
        .count();

    if list_formats > 1 {
        inconsistencies.push(StyleInconsistency {
            inconsistency_type: StyleInconsistencyType::ListFormatMismatch,
            location: "全文".to_string(),
            description: "文档中混用了不同的列表格式".to_string(),
            examples: vec![
                "- 项目".to_string(),
                "* 项目".to_string(),
                "1. 项目".to_string(),
            ],
        });
    }

    // Check for inconsistent date formats
    let has_chinese_date = content.contains("年") && content.contains("月");
    let has_iso_date = content.contains("-") && content.chars().any(|c| c == 'T');

    if has_chinese_date && has_iso_date {
        inconsistencies.push(StyleInconsistency {
            inconsistency_type: StyleInconsistencyType::DateFormatMismatch,
            location: "全文".to_string(),
            description: "文档中混用了不同的日期格式".to_string(),
            examples: vec!["2024年1月1日".to_string(), "2024-01-01".to_string()],
        });
    }

    // Calculate consistency score
    let total_checks = 3; // tone, list, date
    let inconsistent_count = inconsistencies.len();
    let consistency_score = 1.0 - (inconsistent_count as f64 / total_checks as f64);

    // Generate suggestions
    if inconsistencies
        .iter()
        .any(|i| i.inconsistency_type == StyleInconsistencyType::ToneMismatch)
    {
        suggestions.push(StyleSuggestion {
            suggestion: "建议统一文档语气，选择正式或非正式风格贯穿全文".to_string(),
            locations: vec!["全文".to_string()],
            requires_patch: false,
        });
    }

    if inconsistencies
        .iter()
        .any(|i| i.inconsistency_type == StyleInconsistencyType::ListFormatMismatch)
    {
        suggestions.push(StyleSuggestion {
            suggestion: "建议统一列表格式，选择一种格式（- 或 * 或数字）贯穿全文".to_string(),
            locations: vec!["全文".to_string()],
            requires_patch: false,
        });
    }

    StyleConsistencyResult {
        inconsistencies,
        suggestions,
        consistency_score,
    }
}

/// Execute document check.
pub fn execute_document_check(
    input: &DocumentCheckInput,
    evidence: Vec<ContextPacket>,
) -> AppResult<DocumentCheckResult> {
    let request_id = generate_request_id();

    match input.check_type {
        DocumentCheckType::OutlineCheck => {
            let entries = parse_outline(&input.content);
            let outline_result = check_outline(&entries);

            Ok(DocumentCheckResult {
                request_id,
                check_type: input.check_type,
                outline_result: Some(outline_result),
                citation_gap_result: None,
                style_result: None,
                patches: Vec::new(),
                evidence_used: evidence,
                total_tokens: TokenUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                },
            })
        }
        DocumentCheckType::CitationGapCheck => {
            let claims = extract_claims_from_content(&input.content);
            let mut uncited_claims = Vec::new();
            let mut weak_citations = Vec::new();
            let mut suggestions = Vec::new();

            // Use evidence to find supporting citations
            for claim in &claims {
                let support = find_support_for_claim(&claim.statement, &evidence);
                if support.is_empty() {
                    uncited_claims.push(FactClaim {
                        id: claim.id.clone(),
                        statement: claim.statement.clone(),
                        has_support: false,
                        supporting_evidence: Vec::new(),
                        conflicting_evidence: Vec::new(),
                    });
                } else {
                    // Check quality of citation
                    let packet = &evidence[support[0]];
                    if packet.trust_level != crate::ai_runtime::TrustLevel::UserNote {
                        weak_citations.push(WeakCitation {
                            claim: claim.statement.clone(),
                            current_citation: format!(
                                "{} ({})",
                                packet.title, packet.citation_label
                            ),
                            reason: "引用来源可信度不足，建议使用用户笔记作为主要依据".to_string(),
                            suggested_citation: None,
                        });
                    }
                    suggestions.push(CitationSuggestion {
                        claim_id: claim.id.clone(),
                        action: CitationAction::AddCitation,
                        suggested_citation: Some(packet.citation_label.clone()),
                        explanation: format!(
                            "建议将声明「{}」关联到证据 {}",
                            &claim.statement[..claim.statement.len().min(30)],
                            packet.citation_label
                        ),
                    });
                }
            }

            let citation_gap_result = CitationGapCheckResult {
                uncited_claims,
                weak_citations,
                suggestions,
            };

            Ok(DocumentCheckResult {
                request_id,
                check_type: input.check_type,
                outline_result: None,
                citation_gap_result: Some(citation_gap_result),
                style_result: None,
                patches: Vec::new(),
                evidence_used: evidence,
                total_tokens: TokenUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                },
            })
        }
        DocumentCheckType::StyleConsistency => {
            let style_result = analyze_style_consistency(&input.content);

            Ok(DocumentCheckResult {
                request_id,
                check_type: input.check_type,
                outline_result: None,
                citation_gap_result: None,
                style_result: Some(style_result),
                patches: Vec::new(),
                evidence_used: evidence,
                total_tokens: TokenUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                },
            })
        }
        DocumentCheckType::CrossDocReference => {
            // Find cross-document references from evidence
            let refs = analyze_cross_doc_references(&input.target_path, &input.content, &evidence);

            Ok(DocumentCheckResult {
                request_id,
                check_type: input.check_type,
                outline_result: None,
                citation_gap_result: Some(CitationGapCheckResult {
                    uncited_claims: Vec::new(),
                    weak_citations: Vec::new(),
                    suggestions: refs,
                }),
                style_result: None,
                patches: Vec::new(),
                evidence_used: evidence,
                total_tokens: TokenUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                },
            })
        }
    }
}

/// Find supporting evidence indices for a claim.
fn find_support_for_claim(claim: &str, evidence: &[ContextPacket]) -> Vec<usize> {
    let claim_lower = claim.to_lowercase();
    let claim_words: Vec<&str> = claim_lower
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() >= 2)
        .collect();

    if claim_words.is_empty() {
        return vec![];
    }

    evidence
        .iter()
        .enumerate()
        .filter(|(_, packet)| {
            let excerpt_lower = packet.excerpt.to_lowercase();
            let match_count = claim_words
                .iter()
                .filter(|word| excerpt_lower.contains(*word))
                .count();
            match_count as f64 / claim_words.len() as f64 >= 0.3
        })
        .map(|(i, _)| i)
        .collect()
}

/// Analyze cross-document references — suggest notes and evidence from other documents.
fn analyze_cross_doc_references(
    current_path: &str,
    content: &str,
    evidence: &[ContextPacket],
) -> Vec<CitationSuggestion> {
    let mut suggestions = Vec::new();

    // Extract key topics from content
    let topics = extract_key_topics(content);

    // Find evidence from other documents (not the current one)
    for packet in evidence {
        if packet.source_path.as_deref() == Some(current_path) {
            continue; // Skip self-reference
        }

        // Check if any topic matches
        let excerpt_lower = packet.excerpt.to_lowercase();
        let matching_topics: Vec<&str> = topics
            .iter()
            .filter(|topic| excerpt_lower.contains(&topic.to_lowercase()))
            .map(|s| s.as_str())
            .collect();

        if !matching_topics.is_empty() {
            suggestions.push(CitationSuggestion {
                claim_id: generate_claim_id(),
                action: CitationAction::AddCitation,
                suggested_citation: Some(packet.citation_label.clone()),
                explanation: format!(
                    "在「{}」中发现相关内容（主题: {}），建议作为交叉引用",
                    packet.title,
                    matching_topics.join(", ")
                ),
            });
        }
    }

    // Deduplicate and limit
    let mut seen = std::collections::HashSet::new();
    suggestions.retain(|s| {
        let key = s.suggested_citation.clone();
        seen.insert(key)
    });
    suggestions.truncate(10);

    suggestions
}

/// Extract key topics from content.
fn extract_key_topics(content: &str) -> Vec<String> {
    let mut topics = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    // Extract headings as topics
    for line in &lines {
        if line.starts_with('#') {
            let text = line.trim_start_matches('#').trim();
            if !text.is_empty() && text.len() <= 50 {
                topics.push(text.to_string());
            }
        }
    }

    // Extract bold text as topics
    for line in &lines {
        let mut start = 0;
        while let Some(bold_start) = line[start..].find("**") {
            let abs_start = start + bold_start + 2;
            if let Some(bold_end) = line[abs_start..].find("**") {
                let bold_text = &line[abs_start..abs_start + bold_end];
                if bold_text.len() >= 2 && bold_text.len() <= 30 {
                    topics.push(bold_text.to_string());
                }
                start = abs_start + bold_end + 2;
            } else {
                break;
            }
        }
    }

    // Extract named entities (《...》patterns)
    let re = regex::Regex::new(r"《([^》]+)》").unwrap();
    for cap in re.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            topics.push(m.as_str().to_string());
        }
    }

    topics.dedup();
    topics.truncate(15);
    topics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_outline_empty() {
        let content = "Hello, World!";
        let entries = parse_outline(content);
        assert!(entries.is_empty());
    }

    #[test]
    fn test_parse_outline_with_headings() {
        let content = "# Chapter 1\n\nSome content\n\n## Section 1.1\n\nMore content\n\n# Chapter 2\n\nFinal content";
        let entries = parse_outline(content);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].text, "Chapter 1");
        assert_eq!(entries[0].level, 1);
        assert_eq!(entries[1].text, "Section 1.1");
        assert_eq!(entries[1].level, 2);
    }

    #[test]
    fn test_check_outline_skipped_level() {
        let entries = vec![
            OutlineEntry {
                level: 1,
                text: "Chapter".to_string(),
                position: 0,
                word_count: 100,
            },
            OutlineEntry {
                level: 3, // Skipped H2
                text: "Section".to_string(),
                position: 50,
                word_count: 50,
            },
        ];
        let result = check_outline(&entries);
        assert!(!result.issues.is_empty());
        assert_eq!(result.issues[0].issue_type, OutlineIssueType::SkippedLevel);
    }

    #[test]
    fn test_analyze_style_consistency_tone() {
        let content = "请您查看。你可以试试。";
        let result = analyze_style_consistency(content);
        assert!(!result.inconsistencies.is_empty());
        assert_eq!(
            result.inconsistencies[0].inconsistency_type,
            StyleInconsistencyType::ToneMismatch
        );
    }

    #[test]
    fn test_analyze_style_consistency_list() {
        let content = "\n- Item 1\n* Item 2\n1. Item 3";
        let result = analyze_style_consistency(content);
        assert!(!result.inconsistencies.is_empty());
        assert_eq!(
            result.inconsistencies[0].inconsistency_type,
            StyleInconsistencyType::ListFormatMismatch
        );
    }
}
