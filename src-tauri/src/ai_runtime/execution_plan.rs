//! 将 context planner 的输出映射为前端可展示的检索执行计划。

use crate::ai_runtime::context_planner::{ContextPlan, SubQueryType};

/// 单步检索策略（与前端 `RetrievalStep.layer` 对齐）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalLayer {
    Fts,
    Vector,
    Graph,
    Exact,
    Template,
}

/// 检索计划中的单步。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalStepDto {
    pub layer: RetrievalLayer,
    pub query: String,
    pub expected_results: u32,
    pub priority: u32,
}

/// 组装上下文时返回的执行计划摘要。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPlanDto {
    pub steps: Vec<RetrievalStepDto>,
    pub estimated_tokens: u32,
    pub estimated_duration_ms: u32,
}

fn layer_for_sub_query(query_type: &SubQueryType) -> RetrievalLayer {
    match query_type {
        SubQueryType::Original => RetrievalLayer::Fts,
        SubQueryType::RegulationExact => RetrievalLayer::Exact,
        SubQueryType::SemanticExpansion => RetrievalLayer::Vector,
        SubQueryType::Keyword => RetrievalLayer::Fts,
        SubQueryType::Related => RetrievalLayer::Graph,
    }
}

/// 由 `ContextPlan` 生成前端 `ExecutionPlan`。
pub fn execution_plan_from_context_plan(plan: &ContextPlan) -> ExecutionPlanDto {
    let steps: Vec<RetrievalStepDto> = plan
        .sub_queries
        .iter()
        .map(|sq| {
            let priority = u32::from(sq.priority);
            RetrievalStepDto {
                layer: layer_for_sub_query(&sq.query_type),
                query: sq.query.clone(),
                expected_results: priority.max(1),
                priority,
            }
        })
        .collect();

    let estimated_tokens = plan.estimated_tokens.min(u32::MAX as usize) as u32;
    let estimated_duration_ms = estimated_tokens.saturating_mul(3);

    ExecutionPlanDto {
        steps,
        estimated_tokens,
        estimated_duration_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::context_planner::{ContextPlan, QueryIntent, SubQuery, SubQueryType};

    #[test]
    fn maps_sub_queries_to_retrieval_steps() {
        let plan = ContextPlan {
            intent: QueryIntent::General,
            sub_queries: vec![
                SubQuery {
                    query: "民法典 合同".to_string(),
                    query_type: SubQueryType::Keyword,
                    priority: 2,
                },
                SubQuery {
                    query: "相关案例".to_string(),
                    query_type: SubQueryType::Related,
                    priority: 1,
                },
            ],
            estimated_tokens: 1200,
            requires_multi_turn: false,
        };

        let execution = execution_plan_from_context_plan(&plan);
        assert_eq!(execution.steps.len(), 2);
        assert!(matches!(execution.steps[0].layer, RetrievalLayer::Fts));
        assert!(matches!(execution.steps[1].layer, RetrievalLayer::Graph));
        assert_eq!(execution.estimated_tokens, 1200);
        assert_eq!(execution.estimated_duration_ms, 3600);
    }
}
