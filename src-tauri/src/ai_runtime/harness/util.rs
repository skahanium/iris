//! Shared harness helpers.

use crate::ai_runtime::model_gateway::TokenUsage;

pub(crate) fn accumulate_usage(total: &mut TokenUsage, delta: &TokenUsage) {
    total.prompt_tokens += delta.prompt_tokens;
    total.completion_tokens += delta.completion_tokens;
    total.total_tokens += delta.total_tokens;
    total.prompt_cache_hit_tokens += delta.prompt_cache_hit_tokens;
    total.prompt_cache_miss_tokens += delta.prompt_cache_miss_tokens;
}
