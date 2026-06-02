//! Document-level check workflow — handles outline check, citation gap check, style consistency.
//!
//! This module implements document-level analysis:
//! 1. Parse document structure (outline, chapters)
//! 2. Check outline completeness and consistency
//! 3. Check citation gaps (uncited claims)
//! 4. Check style consistency across document
//! 5. Cross-document reference suggestion

use std::sync::LazyLock;

use sha2::{Digest, Sha256};
use tauri::AppHandle;

static RE_REGULATION_BOOK: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"《([^》]+)》").expect("regulation book regex"));

use crate::ai_runtime::model_gateway::{
    GatewayRequest, LlmMessage, MessageRole, ModelGateway, ProviderConfig,
};
use crate::ai_runtime::writing_workflow::build_patch_proposal;
use crate::ai_runtime::{
    AiScene, CitationAction, CitationSuggestion, ContextPacket, FactClaim, PatchProposal,
    SourceSpan, TokenUsage,
};
use crate::error::AppResult;
use crate::storage::db::Database;

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
    /// 基准内容哈希（可选；为空时由 `content` 计算）
    #[serde(default)]
    pub base_content_hash: String,
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
    /// LLM 综合分析摘要（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis_summary: Option<String>,
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
            && trimmed.chars().nth(1).is_some_and(|c| c == '.' || c == ')')
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

            Ok(finish_check_result(
                DocumentCheckResult {
                    request_id,
                    check_type: input.check_type,
                    outline_result: Some(outline_result),
                    citation_gap_result: None,
                    style_result: None,
                    patches: Vec::new(),
                    evidence_used: evidence.clone(),
                    total_tokens: TokenUsage::default(),
                    analysis_summary: None,
                },
                input,
                &evidence,
            ))
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

            Ok(finish_check_result(
                DocumentCheckResult {
                    request_id,
                    check_type: input.check_type,
                    outline_result: None,
                    citation_gap_result: Some(citation_gap_result),
                    style_result: None,
                    patches: Vec::new(),
                    evidence_used: evidence.clone(),
                    total_tokens: TokenUsage::default(),
                    analysis_summary: None,
                },
                input,
                &evidence,
            ))
        }
        DocumentCheckType::StyleConsistency => {
            let style_result = analyze_style_consistency(&input.content);

            Ok(finish_check_result(
                DocumentCheckResult {
                    request_id,
                    check_type: input.check_type,
                    outline_result: None,
                    citation_gap_result: None,
                    style_result: Some(style_result),
                    patches: Vec::new(),
                    evidence_used: evidence.clone(),
                    total_tokens: TokenUsage::default(),
                    analysis_summary: None,
                },
                input,
                &evidence,
            ))
        }
        DocumentCheckType::CrossDocReference => {
            // Find cross-document references from evidence
            let refs = analyze_cross_doc_references(&input.target_path, &input.content, &evidence);

            Ok(finish_check_result(
                DocumentCheckResult {
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
                    evidence_used: evidence.clone(),
                    total_tokens: TokenUsage::default(),
                    analysis_summary: None,
                },
                input,
                &evidence,
            ))
        }
    }
}

fn content_hash_for_input(input: &DocumentCheckInput) -> String {
    if input.base_content_hash.trim().is_empty() {
        crate::cas::hash::content_hash_str(&input.content)
    } else {
        input.base_content_hash.clone()
    }
}

/// 在文档中定位唯一文本片段（用于补丁 range）。
fn find_text_span(content: &str, needle: &str) -> Option<SourceSpan> {
    let needle = needle.trim();
    if needle.is_empty() {
        return None;
    }
    let start = content.find(needle)?;
    if content[start + needle.len()..].contains(needle) {
        return None;
    }
    Some(SourceSpan {
        start,
        end: start + needle.len(),
    })
}

/// 从启发式检查结果生成可确认补丁（引用标注、语气/列表统一等）。
pub fn build_heuristic_document_patches(
    input: &DocumentCheckInput,
    result: &DocumentCheckResult,
    evidence: &[ContextPacket],
) -> Vec<PatchProposal> {
    let hash = content_hash_for_input(input);
    let evidence_ids: Vec<String> = evidence.iter().take(8).map(|p| p.id.clone()).collect();
    let mut patches = Vec::new();

    if let Some(ref citation) = result.citation_gap_result {
        for claim in citation.uncited_claims.iter().take(10) {
            let Some(span) = find_text_span(&input.content, &claim.statement) else {
                continue;
            };
            let original = input.content[span.start..span.end].to_string();
            let label = citation
                .suggestions
                .iter()
                .find(|s| s.claim_id == claim.id)
                .and_then(|s| s.suggested_citation.clone())
                .or_else(|| evidence.first().map(|p| p.citation_label.clone()))
                .unwrap_or_else(|| "待补充依据".to_string());
            let replacement = format!("{original}（依据：{label}）");
            patches.push(build_patch_proposal(
                &input.target_path,
                &hash,
                &original,
                &replacement,
                span,
                evidence_ids.clone(),
            ));
        }
    }

    if let Some(ref style) = result.style_result {
        if style
            .inconsistencies
            .iter()
            .any(|i| i.inconsistency_type == StyleInconsistencyType::ToneMismatch)
        {
            for (from, to) in [("你", "您"), ("咱们", "我们")] {
                if let Some(span) = find_text_span(&input.content, from) {
                    let original = from.to_string();
                    patches.push(build_patch_proposal(
                        &input.target_path,
                        &hash,
                        &original,
                        to,
                        span,
                        evidence_ids.clone(),
                    ));
                }
            }
        }
        if style
            .inconsistencies
            .iter()
            .any(|i| i.inconsistency_type == StyleInconsistencyType::ListFormatMismatch)
        {
            if let Some(pos) = input.content.find("\n* ") {
                let line_start = input.content[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
                let line_end = input.content[line_start..]
                    .find('\n')
                    .map(|i| line_start + i)
                    .unwrap_or(input.content.len());
                let original = input.content[line_start..line_end].to_string();
                if original.starts_with("* ") {
                    let replacement = original.replacen("* ", "- ", 1);
                    patches.push(build_patch_proposal(
                        &input.target_path,
                        &hash,
                        &original,
                        &replacement,
                        SourceSpan {
                            start: line_start,
                            end: line_end,
                        },
                        evidence_ids.clone(),
                    ));
                }
            }
        }
    }

    if let Some(ref outline) = result.outline_result {
        for issue in outline.issues.iter().take(6) {
            if issue.issue_type != OutlineIssueType::SkippedLevel {
                continue;
            }
            let Some(entry) = outline
                .outline_entries
                .iter()
                .find(|e| e.position == issue.position)
            else {
                continue;
            };
            let line_end = input.content[entry.position..]
                .find('\n')
                .map(|i| entry.position + i + 1)
                .unwrap_or(entry.position);
            let insert = format!("\n### {}（建议补充）\n\n", entry.text);
            let original = String::new();
            patches.push(build_patch_proposal(
                &input.target_path,
                &hash,
                &original,
                &insert,
                SourceSpan {
                    start: line_end,
                    end: line_end,
                },
                evidence_ids.clone(),
            ));
        }
    }

    patches
}

#[derive(Debug, serde::Deserialize)]
struct LlmDocumentPatchDraft {
    original_text: String,
    replacement_text: String,
}

/// 解析 LLM 返回的补丁 JSON 数组。
fn parse_llm_document_patches_json(json_str: &str) -> AppResult<Vec<LlmDocumentPatchDraft>> {
    let trimmed = json_str.trim();
    let array_slice = if trimmed.starts_with("```") {
        let start = trimmed.find('[').unwrap_or(0);
        let end = trimmed.rfind(']').unwrap_or(trimmed.len());
        &trimmed[start..=end]
    } else if let Some(start) = trimmed.find('[') {
        let end = trimmed.rfind(']').unwrap_or(trimmed.len());
        &trimmed[start..=end]
    } else {
        trimmed
    };

    let parsed: Vec<serde_json::Value> = serde_json::from_str(array_slice).map_err(|e| {
        crate::error::AppError::msg(format!("failed to parse document patches: {e}"))
    })?;

    Ok(parsed
        .into_iter()
        .filter_map(|v| {
            let original = v["original_text"].as_str()?.trim().to_string();
            let replacement = v["replacement_text"].as_str()?.trim().to_string();
            if original.is_empty() || replacement.is_empty() || original == replacement {
                return None;
            }
            Some(LlmDocumentPatchDraft {
                original_text: original,
                replacement_text: replacement,
            })
        })
        .collect())
}

fn llm_drafts_to_patches(
    input: &DocumentCheckInput,
    drafts: Vec<LlmDocumentPatchDraft>,
    evidence: &[ContextPacket],
) -> Vec<PatchProposal> {
    let hash = content_hash_for_input(input);
    let evidence_ids: Vec<String> = evidence.iter().take(8).map(|p| p.id.clone()).collect();
    let mut patches = Vec::new();

    for draft in drafts.into_iter().take(12) {
        let Some(span) = find_text_span(&input.content, &draft.original_text) else {
            continue;
        };
        let original = input.content[span.start..span.end].to_string();
        patches.push(build_patch_proposal(
            &input.target_path,
            &hash,
            &original,
            &draft.replacement_text,
            span,
            evidence_ids.clone(),
        ));
    }

    patches
}

/// 合并补丁列表，按 `original_text` 去重（保留先出现的项）。
pub fn merge_document_patches(
    mut base: Vec<PatchProposal>,
    extra: Vec<PatchProposal>,
) -> Vec<PatchProposal> {
    let mut seen: std::collections::HashSet<String> =
        base.iter().map(|p| p.original_text.clone()).collect();
    for patch in extra {
        if seen.insert(patch.original_text.clone()) {
            base.push(patch);
        }
    }
    base
}

fn finish_check_result(
    mut result: DocumentCheckResult,
    input: &DocumentCheckInput,
    evidence: &[ContextPacket],
) -> DocumentCheckResult {
    result.patches = build_heuristic_document_patches(input, &result, evidence);
    result
}

/// 调用 LLM 生成最多若干条可定位补丁（`original_text` 必须来自文档原文）。
#[allow(clippy::too_many_arguments)]
async fn generate_llm_document_patches(
    db: &Database,
    app_handle: &AppHandle,
    provider: &ProviderConfig,
    input: &DocumentCheckInput,
    result: &DocumentCheckResult,
    evidence: &[ContextPacket],
) -> AppResult<Vec<PatchProposal>> {
    let rules = ModelGateway::load_active_rules_for_scene(db, AiScene::DraftingAssist)?;
    let system =
        ModelGateway::build_system_prompt(AiScene::DraftingAssist, evidence, &rules, false);
    let heuristic = serialize_heuristic_for_llm(result);
    let excerpt = if input.content.len() > 5000 {
        format!("{}…", &input.content[..5000])
    } else {
        input.content.clone()
    };

    let user = format!(
        "任务：{}\n路径：{}\n\n启发式结果：\n{heuristic}\n\n文档摘录：\n{excerpt}\n\n请输出 JSON 数组（不要代码围栏外的文字），每项：\n{{\"original_text\":\"必须从上文摘录的连续原文\",\"replacement_text\":\"修改后文本\"}}\n\n要求：最多 8 条；original_text 必须在摘录中完全一致出现；不要编造法条；优先修复引用缺口与明显文风问题。",
        check_type_label(input.check_type),
        input.target_path,
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
        max_tokens: Some(3072),
        temperature: Some(0.25),
        stream: false,
    };

    let gateway = ModelGateway::with_defaults(app_handle.clone(), vec![provider.clone()])?;
    let response = gateway.send_request(request).await?;
    let content = response.content.unwrap_or_default();
    let drafts = parse_llm_document_patches_json(&content)?;
    Ok(llm_drafts_to_patches(input, drafts, evidence))
}

fn check_type_label(check_type: DocumentCheckType) -> &'static str {
    match check_type {
        DocumentCheckType::OutlineCheck => "大纲检查",
        DocumentCheckType::CitationGapCheck => "引用缺口检查",
        DocumentCheckType::StyleConsistency => "风格一致性检查",
        DocumentCheckType::CrossDocReference => "跨文档引用建议",
    }
}

fn serialize_heuristic_for_llm(result: &DocumentCheckResult) -> String {
    let mut parts = Vec::new();
    if let Some(ref o) = result.outline_result {
        parts.push(format!(
            "大纲问题 {} 条，建议 {} 条",
            o.issues.len(),
            o.suggestions.len()
        ));
        for issue in o.issues.iter().take(8) {
            parts.push(format!("- [{}] {}", issue.heading_path, issue.description));
        }
    }
    if let Some(ref c) = result.citation_gap_result {
        parts.push(format!(
            "未引用声明 {} 条，弱引用 {} 条",
            c.uncited_claims.len(),
            c.weak_citations.len()
        ));
        for claim in c.uncited_claims.iter().take(5) {
            parts.push(format!("- 缺依据: {}", claim.statement));
        }
    }
    if let Some(ref s) = result.style_result {
        parts.push(format!(
            "风格问题 {} 条，建议 {} 条",
            s.inconsistencies.len(),
            s.suggestions.len()
        ));
    }
    parts.join("\n")
}

/// 在启发式检查结果上叠加 LLM 综合分析。
pub async fn enhance_document_check_with_llm(
    db: &Database,
    app_handle: &AppHandle,
    provider: &ProviderConfig,
    input: &DocumentCheckInput,
    mut result: DocumentCheckResult,
    evidence: &[ContextPacket],
) -> AppResult<DocumentCheckResult> {
    let rules = ModelGateway::load_active_rules_for_scene(db, AiScene::DraftingAssist)?;
    let system =
        ModelGateway::build_system_prompt(AiScene::DraftingAssist, evidence, &rules, false);
    let heuristic = serialize_heuristic_for_llm(&result);
    let excerpt = if input.content.len() > 6000 {
        format!("{}…", &input.content[..6000])
    } else {
        input.content.clone()
    };

    let user = format!(
        "任务：{}\n文档路径：{}\n\n启发式预检结果：\n{heuristic}\n\n文档摘录：\n{excerpt}\n\n请用中文输出：1) 总体评估 2) 优先修复项（编号列表）3) 是否建议用户手动修改（不要编造法条）。不要输出代码围栏。",
        check_type_label(input.check_type),
        input.target_path,
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
        max_tokens: Some(2048),
        temperature: Some(0.3),
        stream: false,
    };

    let gateway = ModelGateway::with_defaults(app_handle.clone(), vec![provider.clone()])?;
    let response = gateway.send_request(request).await?;
    let summary = response.content.unwrap_or_default().trim().to_string();
    result.analysis_summary = if summary.is_empty() {
        None
    } else {
        Some(summary)
    };
    result.total_tokens.prompt_tokens += response.usage.prompt_tokens;
    result.total_tokens.completion_tokens += response.usage.completion_tokens;
    result.total_tokens.total_tokens += response.usage.total_tokens;

    match generate_llm_document_patches(db, app_handle, provider, input, &result, evidence).await {
        Ok(llm_patches) => {
            result.patches = merge_document_patches(result.patches, llm_patches);
        }
        Err(e) => {
            tracing::warn!("Document check LLM patches failed: {e}");
        }
    }

    Ok(result)
}

/// 启发式 + LLM 文档检查。
pub async fn execute_document_check_with_llm(
    db: &Database,
    app_handle: &AppHandle,
    provider: &ProviderConfig,
    input: &DocumentCheckInput,
    evidence: Vec<ContextPacket>,
) -> AppResult<DocumentCheckResult> {
    let base = execute_document_check(input, evidence.clone())?;
    enhance_document_check_with_llm(db, app_handle, provider, input, base, &evidence).await
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

    for cap in RE_REGULATION_BOOK.captures_iter(content) {
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

    #[test]
    fn test_heuristic_citation_patch() {
        let content = "根据法律规定，该企业应当承担相应责任。";
        let claims = extract_claims_from_content(content);
        assert!(!claims.is_empty());
        let input = DocumentCheckInput {
            target_path: "notes/a.md".to_string(),
            content: content.to_string(),
            check_type: DocumentCheckType::CitationGapCheck,
            web_authorized: false,
            base_content_hash: String::new(),
        };
        let citation = CitationGapCheckResult {
            uncited_claims: vec![FactClaim {
                id: "c1".to_string(),
                statement: claims[0].statement.clone(),
                has_support: false,
                supporting_evidence: vec![],
                conflicting_evidence: vec![],
            }],
            weak_citations: vec![],
            suggestions: vec![],
        };
        let result = DocumentCheckResult {
            request_id: "t".to_string(),
            check_type: DocumentCheckType::CitationGapCheck,
            outline_result: None,
            citation_gap_result: Some(citation),
            style_result: None,
            patches: vec![],
            evidence_used: vec![],
            total_tokens: TokenUsage::default(),
            analysis_summary: None,
        };
        let patches = build_heuristic_document_patches(&input, &result, &[]);
        assert_eq!(patches.len(), 1);
        assert!(patches[0].replacement_text.contains("依据"));
    }

    #[test]
    fn test_parse_llm_document_patches_json() {
        let raw = r#"[{"original_text":"你好","replacement_text":"您好"}]"#;
        let drafts = parse_llm_document_patches_json(raw).expect("parse");
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].replacement_text, "您好");
    }

    #[test]
    fn test_merge_document_patches_dedupes() {
        let a = PatchProposal {
            id: "1".to_string(),
            target_path: "x.md".to_string(),
            base_content_hash: "h".to_string(),
            range: SourceSpan { start: 0, end: 1 },
            original_text: "a".to_string(),
            replacement_text: "b".to_string(),
            evidence_packet_ids: vec![],
            risk_level: crate::ai_runtime::RiskLevel::Low,
            warnings: vec![],
            created_at: String::new(),
        };
        let b = a.clone();
        let merged = merge_document_patches(vec![a], vec![b]);
        assert_eq!(merged.len(), 1);
    }
}
