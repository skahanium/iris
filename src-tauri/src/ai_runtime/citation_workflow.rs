//! Citation check workflow — verifies claims and suggests citations.
//!
//! This module implements the `citation_check` workflow:
//! 1. Receive paragraph/selection text, document context, scope, web authorization
//! 2. Extract fact claims and citation needs
//! 3. Search local evidence for support or conflict
//! 4. Optionally search web for external sources
//! 5. Output citation coverage assessment
//! 6. Give suggestions for adding citations or rewriting

use sha2::{Digest, Sha256};

use crate::ai_runtime::{
    CitationAction, CitationCheckInput, CitationCheckResult, CitationCoverage, CitationSuggestion,
    ContextPacket, FactClaim, TokenUsage,
};
use crate::error::AppResult;

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

/// Extract fact claims from paragraph text.
///
/// This is a simplified implementation that splits sentences and identifies
/// potential factual claims based on patterns.
pub fn extract_claims(text: &str) -> Vec<FactClaim> {
    let mut claims = Vec::new();

    // Split by sentence-ending punctuation
    let sentences: Vec<&str> = text
        .split(['。', '！', '？', '.', '!', '?'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && s.len() > 5)
        .collect();

    for sentence in sentences {
        // Check if the sentence looks like a factual claim
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

/// Determine if a sentence is likely a factual claim.
///
/// Heuristics:
/// - Contains numbers or dates
/// - Contains citation markers like "根据", "按照", "依据"
/// - Contains factual verbs like "是", "有", "为"
/// - Is longer than a certain threshold
fn is_likely_claim(text: &str) -> bool {
    let text_lower = text.to_lowercase();

    // Contains numbers (potential statistics, dates, etc.)
    let has_numbers = text.chars().any(|c| c.is_ascii_digit());

    // Contains citation markers
    let has_citation_markers = text_lower.contains("根据")
        || text_lower.contains("按照")
        || text_lower.contains("依据")
        || text_lower.contains("according")
        || text_lower.contains("based on");

    // Contains factual verbs
    let has_factual_verbs = text_lower.contains("是")
        || text_lower.contains("有")
        || text_lower.contains("为")
        || text_lower.contains("was")
        || text_lower.contains("is")
        || text_lower.contains("are");

    // Is reasonably long (not just a phrase)
    let is_long_enough = text.len() >= 10;

    // At least two criteria met
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

/// Find supporting evidence for a claim.
fn find_supporting_evidence(claim: &str, evidence: &[ContextPacket]) -> Vec<String> {
    let claim_lower = claim.to_lowercase();
    let claim_words: Vec<&str> = claim_lower
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() >= 2)
        .collect();

    evidence
        .iter()
        .filter(|packet| {
            let excerpt_lower = packet.excerpt.to_lowercase();
            // Check if any significant words from the claim appear in the excerpt
            let match_count = claim_words
                .iter()
                .filter(|word| excerpt_lower.contains(*word))
                .count();
            // Require at least 30% of claim words to match
            match_count as f64 / claim_words.len() as f64 >= 0.3
        })
        .map(|packet| packet.id.clone())
        .collect()
}

/// Find conflicting evidence for a claim.
fn find_conflicting_evidence(claim: &str, evidence: &[ContextPacket]) -> Vec<String> {
    // Simplified implementation: look for negation patterns
    let claim_lower = claim.to_lowercase();
    let has_negation = claim_lower.contains("不")
        || claim_lower.contains("没有")
        || claim_lower.contains("并非")
        || claim_lower.contains("不是");

    if !has_negation {
        return Vec::new();
    }

    // If claim contains negation, look for evidence that might contradict
    evidence
        .iter()
        .filter(|packet| {
            let excerpt_lower = packet.excerpt.to_lowercase();
            // Simple contradiction detection: claim has negation but excerpt doesn't
            let excerpt_has_negation = excerpt_lower.contains("不")
                || excerpt_lower.contains("没有")
                || excerpt_lower.contains("并非")
                || excerpt_lower.contains("不是");

            !excerpt_has_negation && packet.trust_level == crate::ai_runtime::TrustLevel::UserNote
        })
        .map(|packet| packet.id.clone())
        .collect()
}

/// Assess citation coverage based on claims and evidence.
fn assess_coverage(claims: &[FactClaim]) -> CitationCoverage {
    if claims.is_empty() {
        return CitationCoverage::WellSupported;
    }

    let total = claims.len();
    let supported = claims
        .iter()
        .filter(|c| !c.supporting_evidence.is_empty())
        .count();
    let contradicted = claims
        .iter()
        .filter(|c| !c.conflicting_evidence.is_empty())
        .count();

    let support_ratio = supported as f64 / total as f64;
    let contradiction_ratio = contradicted as f64 / total as f64;

    if contradiction_ratio > 0.3 {
        CitationCoverage::Contradicted
    } else if support_ratio >= 0.8 {
        CitationCoverage::WellSupported
    } else if support_ratio >= 0.5 {
        CitationCoverage::PartiallySupported
    } else if support_ratio >= 0.2 {
        CitationCoverage::WeaklySupported
    } else {
        CitationCoverage::Unsupported
    }
}

/// Generate citation suggestions based on claims and their coverage.
fn generate_suggestions(claims: &[FactClaim]) -> Vec<CitationSuggestion> {
    let mut suggestions = Vec::new();

    for claim in claims {
        if claim.conflicting_evidence.is_empty() && claim.supporting_evidence.is_empty() {
            // No evidence at all - suggest adding citation or rewriting
            suggestions.push(CitationSuggestion {
                claim_id: claim.id.clone(),
                action: CitationAction::AddCitation,
                suggested_citation: None,
                explanation: format!(
                    "声明「{}」缺少引用依据，建议添加来源",
                    &claim.statement[..claim.statement.len().min(50)]
                ),
            });
        } else if !claim.conflicting_evidence.is_empty() {
            // Has conflicting evidence - suggest rewriting or adding qualifier
            suggestions.push(CitationSuggestion {
                claim_id: claim.id.clone(),
                action: CitationAction::AddQualifier,
                suggested_citation: None,
                explanation: format!(
                    "声明「{}」存在冲突证据，建议添加限定词（如可能、通常）",
                    &claim.statement[..claim.statement.len().min(50)]
                ),
            });
        }
    }

    suggestions
}

/// Execute a citation check task.
pub fn execute_citation_check(
    input: &CitationCheckInput,
    evidence: Vec<ContextPacket>,
) -> AppResult<CitationCheckResult> {
    let request_id = uuid::Uuid::new_v4().to_string();

    // Extract claims from the paragraph
    let mut claims = extract_claims(&input.paragraph_text);

    // Find supporting and conflicting evidence for each claim
    for claim in &mut claims {
        claim.supporting_evidence = find_supporting_evidence(&claim.statement, &evidence);
        claim.conflicting_evidence = find_conflicting_evidence(&claim.statement, &evidence);
        claim.has_support = !claim.supporting_evidence.is_empty();
    }

    // Assess overall coverage
    let coverage = assess_coverage(&claims);

    // Generate suggestions
    let suggestions = generate_suggestions(&claims);

    Ok(CitationCheckResult {
        request_id,
        claims,
        coverage,
        suggestions,
        evidence_used: evidence,
        total_tokens: TokenUsage::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_claims() {
        let text = "SQLite 是一个嵌入式数据库。它非常轻量级。根据官方文档，它支持 SQL 标准。";
        let claims = extract_claims(text);
        assert!(!claims.is_empty());
    }

    #[test]
    fn test_is_likely_claim() {
        assert!(is_likely_claim("SQLite 是一个嵌入式数据库"));
        assert!(is_likely_claim("根据官方文档，它支持 SQL 标准"));
        assert!(!is_likely_claim("好的"));
        assert!(!is_likely_claim("嗯"));
    }

    #[test]
    fn test_assess_coverage_empty() {
        let claims = vec![];
        assert!(matches!(
            assess_coverage(&claims),
            CitationCoverage::WellSupported
        ));
    }

    #[test]
    fn test_assess_coverage_unsupported() {
        let claims = vec![FactClaim {
            id: "test".to_string(),
            statement: "test".to_string(),
            has_support: false,
            supporting_evidence: vec![],
            conflicting_evidence: vec![],
        }];
        assert!(matches!(
            assess_coverage(&claims),
            CitationCoverage::Unsupported
        ));
    }

    #[test]
    fn test_assess_coverage_well_supported() {
        let claims = vec![
            FactClaim {
                id: "1".to_string(),
                statement: "test1".to_string(),
                has_support: true,
                supporting_evidence: vec!["evidence1".to_string()],
                conflicting_evidence: vec![],
            },
            FactClaim {
                id: "2".to_string(),
                statement: "test2".to_string(),
                has_support: true,
                supporting_evidence: vec!["evidence2".to_string()],
                conflicting_evidence: vec![],
            },
        ];
        assert!(matches!(
            assess_coverage(&claims),
            CitationCoverage::WellSupported
        ));
    }
}
