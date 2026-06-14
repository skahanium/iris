//! Writing workflow — generates writing suggestions and controlled patches.
//!
//! This module implements the `drafting_assist` scene's workflow:
//! 1. Receive selection text, cursor context, document path, content hash, writing goal
//! 2. Retrieve local evidence (reuses retrieval_broker)
//! 3. Optional web search (reuses search_web)
//! 4. Generate writing suggestions
//! 5. Generate PatchProposal (with original_text, replacement_text, range)
//! 6. Return result (never auto-write)

use sha2::{Digest, Sha256};
use tauri::AppHandle;

use crate::ai_runtime::model_gateway::{
    GatewayRequest, LlmMessage, MessageRole, ModelGateway, ProviderConfig,
};
use crate::ai_runtime::{
    AiScene, ContextPacket, PatchProposal, RiskLevel, SourceSpan, WritingIntent, WritingSuggestion,
};
use crate::error::AppResult;
use crate::storage::db::Database;

/// Writing task result with token usage.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WritingTaskOutput {
    pub request_id: String,
    pub suggestions: Vec<WritingSuggestion>,
    pub patches: Vec<PatchProposal>,
    pub evidence_used: Vec<ContextPacket>,
    pub total_tokens: crate::ai_types::TokenUsage,
}

/// Generate a unique patch ID.
pub fn generate_patch_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut hasher = Sha256::new();
    hasher.update(timestamp.to_be_bytes());
    let hash = format!("{:x}", hasher.finalize());
    format!("patch-{}", &hash[..12])
}

/// Truncate excerpt for prompt context without splitting multibyte UTF-8.
fn truncate_excerpt_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max_chars).collect::<String>())
    }
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

/// Compute SHA-256 hash of content.
pub fn compute_content_hash(content: &str) -> String {
    crate::cas::hash::content_hash_str(content)
}

/// Assess risk level based on the patch characteristics.
fn assess_risk_level(original: &str, replacement: &str) -> RiskLevel {
    let original_len = original.len();
    let replacement_len = replacement.len();
    let size_ratio = if original_len > 0 {
        replacement_len as f64 / original_len as f64
    } else {
        replacement_len as f64
    };

    // Large replacements or significant size changes are higher risk
    if replacement_len > 500 || !(0.3..=3.0).contains(&size_ratio) {
        RiskLevel::High
    } else if replacement_len > 100 || !(0.5..=2.0).contains(&size_ratio) {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    }
}

/// Generate warnings for a patch.
fn generate_warnings(original: &str, replacement: &str, risk_level: RiskLevel) -> Vec<String> {
    let mut warnings = Vec::new();

    if risk_level == RiskLevel::High {
        warnings.push("高风险补丁：修改范围较大，请仔细检查".to_string());
    }

    if replacement.is_empty() && !original.is_empty() {
        warnings.push("此补丁将删除选中文本".to_string());
    }

    if original.is_empty() && !replacement.is_empty() {
        warnings.push("此补丁将在光标位置插入新文本".to_string());
    }

    // Check for markdown structure changes
    let original_headers = original.matches('#').count();
    let replacement_headers = replacement.matches('#').count();
    if replacement_headers > original_headers {
        warnings.push("补丁包含新的标题，可能影响文档结构".to_string());
    }

    warnings
}

/// Build a PatchProposal from original text, replacement, and context.
pub fn build_patch_proposal(
    target_path: &str,
    base_content_hash: &str,
    original_text: &str,
    replacement_text: &str,
    range: SourceSpan,
    evidence_packet_ids: Vec<String>,
) -> PatchProposal {
    let risk_level = assess_risk_level(original_text, replacement_text);
    let warnings = generate_warnings(original_text, replacement_text, risk_level);

    PatchProposal {
        id: generate_patch_id(),
        target_path: target_path.to_string(),
        base_content_hash: base_content_hash.to_string(),
        range,
        original_text: original_text.to_string(),
        replacement_text: replacement_text.to_string(),
        evidence_packet_ids,
        risk_level,
        warnings,
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
    }
}

/// Detect writing intent from the writing goal and context.
pub fn detect_writing_intent(goal: &str, selection: Option<&str>) -> WritingIntent {
    let goal_lower = goal.to_lowercase();

    if goal_lower.contains("续写")
        || goal_lower.contains("继续")
        || goal_lower.contains("接着写")
        || goal_lower.contains("continue")
    {
        WritingIntent::Continue
    } else if goal_lower.contains("改写")
        || goal_lower.contains("重写")
        || goal_lower.contains("换个说法")
        || goal_lower.contains("rewrite")
    {
        WritingIntent::Rewrite
    } else if goal_lower.contains("依据")
        || goal_lower.contains("引用")
        || goal_lower.contains("证据")
        || goal_lower.contains("evidence")
        || goal_lower.contains("citation")
    {
        WritingIntent::AddEvidence
    } else if goal_lower.contains("提纲")
        || goal_lower.contains("大纲")
        || goal_lower.contains("结构")
        || goal_lower.contains("outline")
    {
        WritingIntent::Outline
    } else if goal_lower.contains("语气")
        || goal_lower.contains("风格")
        || goal_lower.contains("tone")
        || goal_lower.contains("style")
    {
        WritingIntent::UnifyTone
    } else if selection.is_some() {
        // If there's a selection, default to rewrite
        WritingIntent::Rewrite
    } else {
        // No selection, default to continue
        WritingIntent::Continue
    }
}

fn intent_instruction(intent: &WritingIntent) -> &'static str {
    match intent {
        WritingIntent::Continue => "在选区后续写，保持语气一致，不要重复已有内容。",
        WritingIntent::Rewrite => "改写选区，使表达更清晰准确。",
        WritingIntent::AddEvidence => {
            "为选区补充可引用的依据表述（使用证据包中的 citation_label）。"
        }
        WritingIntent::Outline => "根据上下文生成简短提纲（Markdown 列表）。",
        WritingIntent::UnifyTone => "统一选区与上下文的语气风格。",
        _ => "根据写作目标修改或续写选区。",
    }
}

/// 调用 LLM 生成替换文本（仅返回正文，不包裹解释）。
#[allow(clippy::too_many_arguments)]
pub async fn generate_replacement_with_llm(
    db: &Database,
    app_handle: &AppHandle,
    provider: &ProviderConfig,
    intent: &WritingIntent,
    selection: &str,
    cursor_context: &str,
    goal: &str,
    evidence: &[ContextPacket],
) -> AppResult<(String, crate::ai_types::TokenUsage)> {
    let rules = ModelGateway::load_active_rules_for_scene(db, AiScene::DraftingAssist)?;
    let profile = crate::ai_runtime::prompt_profile::PromptProfile::load(db).unwrap_or_default();
    let system = ModelGateway::build_system_prompt_with_profile(
        AiScene::DraftingAssist,
        evidence,
        &rules,
        false,
        &profile,
    );

    let evidence_block = if evidence.is_empty() {
        "（无额外证据包）".to_string()
    } else {
        evidence
            .iter()
            .take(12)
            .map(|p| {
                format!(
                    "{} {} — {}",
                    p.citation_label,
                    p.title,
                    truncate_excerpt_chars(&p.excerpt, 200)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let user = format!(
        "{}\n\n写作目标：{goal}\n\n光标邻域上下文：\n{cursor_context}\n\n待处理选区：\n{selection}\n\n可用证据：\n{evidence_block}\n\n请只输出替换后的 Markdown 文本（用于直接替换选区），不要输出解释或代码围栏。",
        intent_instruction(intent),
    );

    let request = GatewayRequest {
        provider: provider.clone(),
        messages: vec![
            LlmMessage {
                role: MessageRole::System,
                content: system.into(),
                tool_call_id: None,
                tool_calls: None,

                ..Default::default()
            },
            LlmMessage {
                role: MessageRole::User,
                content: user.into(),
                tool_call_id: None,
                tool_calls: None,

                ..Default::default()
            },
        ],
        tools: vec![],
        max_tokens: Some(2048),
        temperature: Some(0.4),
        stream: false,
        thinking: false,
        skip_stub_ids: vec![],
    };

    let gateway = ModelGateway::with_defaults(app_handle.clone(), vec![provider.clone()])?;
    let response = gateway.send_request(request).await?;
    let usage = response.usage;
    let mut text = response.content.unwrap_or_default().trim().to_string();
    if text.starts_with("```") && text.rfind("```").is_some() {
        let inner = text
            .trim_start_matches('`')
            .trim_start_matches(|c: char| c.is_alphanumeric() || c == '\n')
            .trim();
        text = inner.trim_end_matches('`').trim().to_string();
    }
    if text.is_empty() {
        text = selection.to_string();
    }
    Ok((
        text,
        crate::ai_types::TokenUsage {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            ..Default::default()
        },
    ))
}

/// Build a writing suggestion.
pub fn build_writing_suggestion(
    intent: WritingIntent,
    explanation: &str,
    confidence: f64,
) -> WritingSuggestion {
    WritingSuggestion {
        id: generate_suggestion_id(),
        intent,
        explanation: explanation.to_string(),
        confidence,
    }
}

/// Validate a patch proposal against current content.
pub fn validate_patch(
    patch: &PatchProposal,
    current_content: &str,
) -> Result<(), crate::ai_runtime::PatchValidationError> {
    // Check content hash
    let current_hash = compute_content_hash(current_content);
    if current_hash != patch.base_content_hash {
        return Err(crate::ai_runtime::PatchValidationError::HashMismatch {
            expected: patch.base_content_hash.clone(),
            actual: current_hash,
        });
    }

    // Check range bounds
    let content_len = current_content.len();
    if patch.range.start > content_len || patch.range.end > content_len {
        return Err(crate::ai_runtime::PatchValidationError::RangeOutOfBounds {
            range_start: patch.range.start,
            range_end: patch.range.end,
            content_length: content_len,
        });
    }

    // Check original text matches
    let actual_original = &current_content[patch.range.start..patch.range.end];
    if actual_original != patch.original_text {
        return Err(crate::ai_runtime::PatchValidationError::TextMismatch {
            expected: patch.original_text.clone(),
            actual: actual_original.to_string(),
        });
    }

    Ok(())
}

/// Apply a patch to content and return the new content.
pub fn apply_patch(patch: &PatchProposal, current_content: &str) -> AppResult<String> {
    // Validate first
    validate_patch(patch, current_content)
        .map_err(|e| crate::error::AppError::msg(format!("补丁验证失败: {e}")))?;

    // Apply the patch
    let mut new_content =
        String::with_capacity(current_content.len() + patch.replacement_text.len());
    new_content.push_str(&current_content[..patch.range.start]);
    new_content.push_str(&patch.replacement_text);
    new_content.push_str(&current_content[patch.range.end..]);

    Ok(new_content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_patch_id() {
        let id1 = generate_patch_id();
        let id2 = generate_patch_id();
        assert_ne!(id1, id2);
        assert!(id1.starts_with("patch-"));
    }

    #[test]
    fn truncate_excerpt_chars_respects_unicode_boundaries() {
        let s = "第三条 各级监察委员会是行使国家监察职能的专责机关,依照本法对所有行使公权力的公职人员(以下称公职人员)进行监察,调查职务违法和职务犯罪,开展廉政建设和反腐败工作,维护宪法和法律的尊严。";
        let truncated = truncate_excerpt_chars(s, 50);
        assert!(truncated.chars().count() <= 51);
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn test_compute_content_hash() {
        let hash1 = compute_content_hash("hello");
        let hash2 = compute_content_hash("hello");
        let hash3 = compute_content_hash("world");
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_assess_risk_level() {
        assert_eq!(assess_risk_level("short", "also short"), RiskLevel::Low);
        // 100 chars replacement for 1 char original -> High risk (ratio > 3.0)
        assert_eq!(assess_risk_level("a", &"b".repeat(100)), RiskLevel::High);
        // 600 chars replacement -> High risk
        assert_eq!(assess_risk_level("a", &"b".repeat(600)), RiskLevel::High);
        // Medium risk: 50 chars replacement for 50 chars original (ratio = 1.0, len > 100 is false)
        // Actually, 50 chars is not > 100, so it should be Low
        assert_eq!(
            assess_risk_level(&"a".repeat(50), &"b".repeat(50)),
            RiskLevel::Low
        );
        // Medium risk: 150 chars replacement (len > 100)
        assert_eq!(
            assess_risk_level(&"a".repeat(50), &"b".repeat(150)),
            RiskLevel::Medium
        );
    }

    #[test]
    fn test_detect_writing_intent() {
        assert!(matches!(
            detect_writing_intent("请续写这段文字", None),
            WritingIntent::Continue
        ));
        assert!(matches!(
            detect_writing_intent("改写这个段落", Some("selected")),
            WritingIntent::Rewrite
        ));
        assert!(matches!(
            detect_writing_intent("添加引用依据", None),
            WritingIntent::AddEvidence
        ));
    }

    #[test]
    fn test_validate_patch_success() {
        let content = "Hello, World!";
        let hash = compute_content_hash(content);
        let patch = PatchProposal {
            id: "test".to_string(),
            target_path: "test.md".to_string(),
            base_content_hash: hash,
            range: SourceSpan { start: 7, end: 12 },
            original_text: "World".to_string(),
            replacement_text: "Rust".to_string(),
            evidence_packet_ids: vec![],
            risk_level: RiskLevel::Low,
            warnings: vec![],
            created_at: "".to_string(),
        };
        assert!(validate_patch(&patch, content).is_ok());
    }

    #[test]
    fn test_validate_patch_hash_mismatch() {
        let content = "Hello, World!";
        let patch = PatchProposal {
            id: "test".to_string(),
            target_path: "test.md".to_string(),
            base_content_hash: "wrong_hash".to_string(),
            range: SourceSpan { start: 7, end: 12 },
            original_text: "World".to_string(),
            replacement_text: "Rust".to_string(),
            evidence_packet_ids: vec![],
            risk_level: RiskLevel::Low,
            warnings: vec![],
            created_at: "".to_string(),
        };
        assert!(matches!(
            validate_patch(&patch, content),
            Err(crate::ai_runtime::PatchValidationError::HashMismatch { .. })
        ));
    }

    #[test]
    fn test_apply_patch() {
        let content = "Hello, World!";
        let hash = compute_content_hash(content);
        let patch = PatchProposal {
            id: "test".to_string(),
            target_path: "test.md".to_string(),
            base_content_hash: hash,
            range: SourceSpan { start: 7, end: 12 },
            original_text: "World".to_string(),
            replacement_text: "Rust".to_string(),
            evidence_packet_ids: vec![],
            risk_level: RiskLevel::Low,
            warnings: vec![],
            created_at: "".to_string(),
        };
        let result = apply_patch(&patch, content).unwrap();
        assert_eq!(result, "Hello, Rust!");
    }
}
