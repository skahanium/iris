//! Context Planner — query intent detection and sub-query generation.
//!
//! Analyzes user query to determine intent and generate appropriate
//! retrieval sub-queries for the retrieval broker.

use crate::ai_runtime::agent_task_policy::AgentTaskPolicy;
use crate::ai_runtime::AiScene;
use crate::ai_types::AgentIntent;
use crate::error::AppResult;
use serde::{Deserialize, Serialize};

// ─── Intent Types ────────────────────────────────────────

/// Detected intent from user query.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryIntent {
    /// 查找特定法规条款
    RegulationLookup {
        regulation_name: Option<String>,
        article: Option<String>,
    },
    /// 查找笔记内容
    NoteSearch { keywords: Vec<String> },
    /// 查找关联材料
    RelatedMaterials { topic: String },
    /// 写作辅助请求
    WritingAssist {
        assist_type: WritingAssistType,
        context: String,
    },
    /// 研究分析请求
    ResearchAnalysis { sub_queries: Vec<String> },
    /// 通用查询
    General,
}

/// Writing assistance sub-types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WritingAssistType {
    /// 结构建议
    StructureSuggestion,
    /// 段落生成
    ParagraphGeneration,
    /// 改写润色
    Rewrite,
    /// 法规引用建议
    CitationSuggestion,
    /// 一致性检查
    ConsistencyCheck,
}

/// Sub-query for retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubQuery {
    pub query: String,
    pub query_type: SubQueryType,
    pub priority: u8,
}

/// Sub-query types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubQueryType {
    /// 原始查询
    Original,
    /// 法规精确查询
    RegulationExact,
    /// 语义扩展查询
    SemanticExpansion,
    /// 关键词查询
    Keyword,
    /// 关联查询
    Related,
}

/// Context plan with intent and sub-queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPlan {
    pub intent: QueryIntent,
    pub sub_queries: Vec<SubQuery>,
    pub estimated_tokens: usize,
    pub requires_multi_turn: bool,
}

// ─── Context Planner ─────────────────────────────────────

/// Analyze query and generate context plan.
pub fn plan_context(
    query: &str,
    scene: AiScene,
    note_path: Option<&str>,
) -> AppResult<ContextPlan> {
    let intent = detect_intent(query, scene);
    let sub_queries = generate_sub_queries(&intent, query, note_path);
    let estimated_tokens = estimate_tokens(&sub_queries);
    let requires_multi_turn = matches!(scene, AiScene::ResearchSynthesis);

    Ok(ContextPlan {
        intent,
        sub_queries,
        estimated_tokens,
        requires_multi_turn,
    })
}

/// Analyze query and generate context plan from task policy.
pub fn plan_context_for_policy(
    query: &str,
    policy: &AgentTaskPolicy,
    note_path: Option<&str>,
) -> AppResult<ContextPlan> {
    let intent = detect_intent_for_policy(query, policy);
    let sub_queries = generate_sub_queries_for_policy(&intent, query, policy.scope, note_path);
    let estimated_tokens = estimate_tokens(&sub_queries);
    let requires_multi_turn = matches!(
        policy.intent,
        AgentIntent::Research | AgentIntent::CitationCheck | AgentIntent::DocumentCheck
    );

    Ok(ContextPlan {
        intent,
        sub_queries,
        estimated_tokens,
        requires_multi_turn,
    })
}

fn detect_intent_for_policy(query: &str, policy: &AgentTaskPolicy) -> QueryIntent {
    let lower = query.to_lowercase();

    if let Some(regulation_intent) = detect_regulation_intent(&lower) {
        return regulation_intent;
    }

    if matches!(
        policy.intent,
        AgentIntent::RewriteSelection
            | AgentIntent::Write
            | AgentIntent::Chapter
            | AgentIntent::DocumentCheck
    ) {
        if let Some(writing_intent) = detect_writing_intent(&lower, query) {
            return writing_intent;
        }
    }

    if matches!(
        policy.intent,
        AgentIntent::Research | AgentIntent::CitationCheck
    ) {
        if let Some(research_intent) = detect_research_intent(query) {
            return research_intent;
        }
    }

    note_search_or_general(query, &lower)
}

fn note_search_or_general(query: &str, lower: &str) -> QueryIntent {
    if !lower.is_empty() {
        let keywords: Vec<String> = query
            .split(|c: char| c.is_whitespace() || c == '，' || c == '。' || c == '？' || c == '！')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        if !keywords.is_empty() {
            QueryIntent::NoteSearch { keywords }
        } else {
            QueryIntent::General
        }
    } else {
        QueryIntent::General
    }
}

/// Detect intent from query text.
fn detect_intent(query: &str, scene: AiScene) -> QueryIntent {
    let lower = query.to_lowercase();

    // Check for regulation patterns
    if let Some(regulation_intent) = detect_regulation_intent(&lower) {
        return regulation_intent;
    }

    // Check for writing assist patterns
    if matches!(scene, AiScene::DraftingAssist) {
        if let Some(writing_intent) = detect_writing_intent(&lower, query) {
            return writing_intent;
        }
    }

    // Check for research patterns
    if matches!(scene, AiScene::ResearchSynthesis) {
        if let Some(research_intent) = detect_research_intent(query) {
            return research_intent;
        }
    }

    // Default to general or note search
    note_search_or_general(query, &lower)
}

/// Detect regulation lookup intent.
fn detect_regulation_intent(lower: &str) -> Option<QueryIntent> {
    // Regulation patterns require structural markers, not just keyword mentions.
    // "法规" alone is too broad (e.g. "法规依据" is a writing assist request).
    let structural_patterns = ["条例", "规定", "办法", "细则", "准则", "规范"];
    let has_structural_keyword = structural_patterns.iter().any(|p| lower.contains(p));
    let has_article = lower.contains("第") && (lower.contains("条") || lower.contains("款"));
    let has_explicit_ref = lower.contains("《") && lower.contains("》");

    if has_structural_keyword || has_article || has_explicit_ref {
        // Extract regulation name if possible
        let regulation_name = extract_regulation_name(lower);
        let article = extract_article(lower);

        return Some(QueryIntent::RegulationLookup {
            regulation_name,
            article,
        });
    }

    None
}

/// Extract regulation name from query.
fn extract_regulation_name(query: &str) -> Option<String> {
    // Try to extract "XX条例" pattern
    let patterns = ["条例", "法规", "规定", "办法", "细则"];

    for pattern in patterns {
        if let Some(byte_pos) = query.find(pattern) {
            // Find the char index of the pattern start
            let char_pos = query[..byte_pos].chars().count();
            // Look backwards for the start of the name (whitespace, 的, 中)
            let chars: Vec<char> = query.chars().collect();
            let mut start = 0usize;
            for i in (0..char_pos).rev() {
                if chars[i].is_whitespace() || chars[i] == '的' || chars[i] == '中' {
                    start = i + 1;
                    break;
                }
            }
            let pattern_char_len = pattern.chars().count();
            let name: String = chars[start..char_pos + pattern_char_len].iter().collect();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }

    None
}

/// Extract article number from query.
fn extract_article(query: &str) -> Option<String> {
    // Pattern: 第X条, 第X款
    if let Some(byte_pos) = query.find('第') {
        let after = &query[byte_pos..];
        if let Some(end_byte) = after.find(['条', '款', '项']) {
            // Use char boundary-safe slicing
            let article: String = after
                .chars()
                .take(after[..end_byte].chars().count() + 1)
                .collect();
            return Some(article);
        }
    }
    None
}

/// Detect writing assist intent.
fn detect_writing_intent(lower: &str, original: &str) -> Option<QueryIntent> {
    let assist_type = if lower.contains("结构") || lower.contains("大纲") || lower.contains("框架")
    {
        Some(WritingAssistType::StructureSuggestion)
    } else if lower.contains("生成") || lower.contains("写一段") || lower.contains("续写") {
        Some(WritingAssistType::ParagraphGeneration)
    } else if lower.contains("改写") || lower.contains("润色") || lower.contains("优化") {
        Some(WritingAssistType::Rewrite)
    } else if lower.contains("引用") || lower.contains("依据") || lower.contains("法规") {
        Some(WritingAssistType::CitationSuggestion)
    } else if lower.contains("检查") || lower.contains("一致") || lower.contains("规范") {
        Some(WritingAssistType::ConsistencyCheck)
    } else {
        None
    };

    assist_type.map(|t| QueryIntent::WritingAssist {
        assist_type: t,
        context: original.to_string(),
    })
}

/// Detect research intent.
fn detect_research_intent(query: &str) -> Option<QueryIntent> {
    // Research queries often contain comparative or analytical language
    let research_patterns = [
        "比较", "对比", "分析", "论证", "证据", "观点", "立场", "原因", "影响", "关系", "区别",
        "联系",
    ];

    let has_research_pattern = research_patterns.iter().any(|p| query.contains(p));

    if has_research_pattern {
        // Generate sub-queries for research
        let sub_queries = vec![
            query.to_string(),
            format!("{} 相关规定", query),
            format!("{} 案例", query),
        ];

        return Some(QueryIntent::ResearchAnalysis { sub_queries });
    }

    None
}

/// Generate sub-queries based on intent.
fn generate_sub_queries(
    intent: &QueryIntent,
    original_query: &str,
    _note_path: Option<&str>,
) -> Vec<SubQuery> {
    let mut queries = vec![SubQuery {
        query: original_query.to_string(),
        query_type: SubQueryType::Original,
        priority: 10,
    }];

    match intent {
        QueryIntent::RegulationLookup {
            regulation_name,
            article,
        } => {
            // Add exact regulation query
            if let (Some(name), Some(art)) = (regulation_name, article) {
                queries.push(SubQuery {
                    query: format!("{} {}", name, art),
                    query_type: SubQueryType::RegulationExact,
                    priority: 15,
                });
            }

            // Add keyword query
            if let Some(name) = regulation_name {
                queries.push(SubQuery {
                    query: name.clone(),
                    query_type: SubQueryType::Keyword,
                    priority: 12,
                });
            }
        }

        QueryIntent::WritingAssist {
            assist_type,
            context,
        } => {
            match assist_type {
                WritingAssistType::CitationSuggestion => {
                    // Search for relevant regulations
                    queries.push(SubQuery {
                        query: format!("{} 法规依据", context),
                        query_type: SubQueryType::SemanticExpansion,
                        priority: 12,
                    });
                }
                WritingAssistType::StructureSuggestion => {
                    // Search for similar document structures
                    queries.push(SubQuery {
                        query: format!("{} 结构模板", context),
                        query_type: SubQueryType::SemanticExpansion,
                        priority: 11,
                    });
                }
                _ => {}
            }
        }

        QueryIntent::ResearchAnalysis { sub_queries } => {
            // Add research sub-queries
            for (i, sq) in sub_queries.iter().enumerate().skip(1) {
                queries.push(SubQuery {
                    query: sq.clone(),
                    query_type: SubQueryType::SemanticExpansion,
                    priority: (10 - i as u8).max(5),
                });
            }
        }

        _ => {}
    }

    // 不在打开笔记时自动追加图谱子查询，避免简单对话弹出「检索计划」且与 Harness 工具重复。

    // Sort by priority (highest first)
    queries.sort_by(|a, b| b.priority.cmp(&a.priority));

    queries
}

fn generate_sub_queries_for_policy(
    intent: &QueryIntent,
    original_query: &str,
    scope: crate::ai_runtime::agent_task_policy::AgentTaskScope,
    note_path: Option<&str>,
) -> Vec<SubQuery> {
    let mut queries = generate_sub_queries(intent, original_query, note_path);
    if matches!(
        scope,
        crate::ai_runtime::agent_task_policy::AgentTaskScope::Note
            | crate::ai_runtime::agent_task_policy::AgentTaskScope::Selection
    ) {
        if let Some(path) = note_path {
            queries.push(SubQuery {
                query: format!("{original_query} 当前笔记 {path}"),
                query_type: SubQueryType::Related,
                priority: 6,
            });
        }
    }
    queries
}

/// Estimate token count for sub-queries.
fn estimate_tokens(sub_queries: &[SubQuery]) -> usize {
    // Rough estimate: 1 token per 2 characters for Chinese
    sub_queries
        .iter()
        .map(|sq| sq.query.chars().count() / 2)
        .sum::<usize>()
        .max(100) // Minimum 100 tokens
}

// ─── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_regulation_intent_basic() {
        let intent = detect_intent("纪律处分条例第三条", AiScene::KnowledgeLookup);
        assert!(matches!(intent, QueryIntent::RegulationLookup { .. }));
    }

    #[test]
    fn detect_writing_intent_citation() {
        let intent = detect_intent("帮我找一下这段话的法规依据", AiScene::DraftingAssist);
        assert!(matches!(
            intent,
            QueryIntent::WritingAssist {
                assist_type: WritingAssistType::CitationSuggestion,
                ..
            }
        ));
    }

    #[test]
    fn generate_sub_queries_for_regulation() {
        let intent = QueryIntent::RegulationLookup {
            regulation_name: Some("纪律处分条例".to_string()),
            article: Some("第三条".to_string()),
        };

        let queries = generate_sub_queries(&intent, "纪律处分条例第三条", None);

        // Should have original + exact regulation + keyword
        assert!(queries.len() >= 2);
        assert!(queries
            .iter()
            .any(|q| matches!(q.query_type, SubQueryType::RegulationExact)));
    }

    #[test]
    fn plan_context_returns_valid_plan() {
        let plan = plan_context("违反组织纪律的处分规定", AiScene::KnowledgeLookup, None).unwrap();

        assert!(!plan.sub_queries.is_empty());
        assert!(plan.estimated_tokens > 0);
    }

    #[test]
    fn open_note_does_not_add_graph_sub_query() {
        let plan = plan_context(
            "今天是什么日子",
            AiScene::KnowledgeLookup,
            Some("notes/a.md"),
        )
        .unwrap();
        assert_eq!(plan.sub_queries.len(), 1);
        assert!(!plan
            .sub_queries
            .iter()
            .any(|q| matches!(q.query_type, SubQueryType::Related)));
    }
}
