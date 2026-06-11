import { describe, expect, it } from "vitest";

import { accumulateTokenUsage } from "@/lib/token-usage";
import type { TokenUsage } from "@/types/ai";

describe("accumulateTokenUsage", () => {
  it("adds provider usage into an empty session total", () => {
    const delta: TokenUsage = {
      prompt_tokens: 12,
      completion_tokens: 7,
      total_tokens: 19,
      prompt_cache_hit_tokens: 3,
      prompt_cache_miss_tokens: 9,
    };

    expect(accumulateTokenUsage(null, delta)).toEqual(delta);
  });

  it("preserves optional cache counters when later usage omits them", () => {
    const previous: TokenUsage = {
      prompt_tokens: 10,
      completion_tokens: 4,
      total_tokens: 14,
      prompt_cache_hit_tokens: 6,
      prompt_cache_miss_tokens: 2,
    };
    const delta: TokenUsage = {
      prompt_tokens: 2,
      completion_tokens: 8,
      total_tokens: 10,
    };

    expect(accumulateTokenUsage(previous, delta)).toEqual({
      prompt_tokens: 12,
      completion_tokens: 12,
      total_tokens: 24,
      prompt_cache_hit_tokens: 6,
      prompt_cache_miss_tokens: 2,
    });
  });
});
