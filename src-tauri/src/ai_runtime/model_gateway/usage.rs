use crate::ai_types::TokenUsage;

pub(super) fn parse_usage(json: &serde_json::Value) -> TokenUsage {
    let usage = &json["usage"];
    TokenUsage {
        prompt_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0) as u32,
        completion_tokens: usage["completion_tokens"].as_u64().unwrap_or(0) as u32,
        total_tokens: usage["total_tokens"].as_u64().unwrap_or(0) as u32,
        prompt_cache_hit_tokens: usage["prompt_cache_hit_tokens"].as_u64().unwrap_or(0) as u32,
        prompt_cache_miss_tokens: usage["prompt_cache_miss_tokens"].as_u64().unwrap_or(0) as u32,
    }
}
