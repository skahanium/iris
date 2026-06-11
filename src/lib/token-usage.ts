import type { TokenUsage } from "@/types/ai";

export function accumulateTokenUsage(
  previous: TokenUsage | null,
  delta: TokenUsage,
): TokenUsage {
  return {
    prompt_tokens: (previous?.prompt_tokens ?? 0) + delta.prompt_tokens,
    completion_tokens:
      (previous?.completion_tokens ?? 0) + delta.completion_tokens,
    total_tokens: (previous?.total_tokens ?? 0) + delta.total_tokens,
    prompt_cache_hit_tokens:
      (previous?.prompt_cache_hit_tokens ?? 0) +
      (delta.prompt_cache_hit_tokens ?? 0),
    prompt_cache_miss_tokens:
      (previous?.prompt_cache_miss_tokens ?? 0) +
      (delta.prompt_cache_miss_tokens ?? 0),
  };
}
