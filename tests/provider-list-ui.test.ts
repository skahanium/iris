import { describe, expect, it } from "vitest";

import {
  mcpListDotAriaLabel,
  mcpListDotTone,
  mcpListMappingShortLabel,
} from "@/components/ai/skills/mcpProviderListUi";
import {
  providerIcon,
  llmModelShowsVisionBadge,
} from "@/components/settings/llmProviderListUi";
import type { LlmEnabledProviderModel } from "@/components/settings/llmProviderTypes";
import { Bot, Brain, Settings2, Sparkles } from "lucide-react";

describe("provider list UI helpers", () => {
  it("maps known LLM provider ids to lucide icons", () => {
    expect(providerIcon("openai")).toBe(Sparkles);
    expect(providerIcon("anthropic")).toBe(Brain);
    expect(providerIcon("custom_foo")).toBe(Settings2);
    expect(providerIcon("unknown")).toBe(Bot);
  });

  it("derives MCP list dot tone from enabled and mapping status", () => {
    expect(mcpListDotTone({ enabled: false, mappingStatus: "complete" })).toBe(
      "muted",
    );
    expect(mcpListDotTone({ enabled: true, mappingStatus: "complete" })).toBe(
      "success",
    );
    expect(mcpListDotTone({ enabled: true, mappingStatus: "partial" })).toBe(
      "warning",
    );
    expect(
      mcpListDotAriaLabel({ enabled: false, mappingStatus: "missing" }),
    ).toBe("未启用");
    expect(mcpListMappingShortLabel("complete")).toBe("映射完整");
  });

  it("shows vision badge only after Iris vision probe, not catalog", () => {
    const base = {
      id: "gpt-4o",
      catalog: {
        providerId: "openai",
        id: "gpt-4o",
        displayName: "GPT-4o",
        contextWindow: 128_000,
        maxOutput: 4096,
        supportsTools: true,
        supportsThinking: false,
        supportsVision: true,
        supportsStreaming: true,
        cacheFriendly: false,
        endpointFamily: "open_ai_compatible_chat_completions",
        probeStrategy: "open_ai_models_then_chat",
      },
      registry: {
        providerId: "openai",
        modelId: "gpt-4o",
        displayName: "GPT-4o",
        source: "manual",
        stale: false,
        firstSeenAt: null,
        lastSeenAt: null,
        lastRefreshedAt: null,
        textVerifiedAt: "2026-01-01T00:00:00Z",
        visionVerifiedAt: "built_in",
      },
    } satisfies LlmEnabledProviderModel;

    expect(llmModelShowsVisionBadge(base)).toBe(false);

    expect(
      llmModelShowsVisionBadge({
        ...base,
        registry: {
          ...base.registry!,
          visionVerifiedAt: "2026-01-02T00:00:00Z",
        },
      }),
    ).toBe(true);
  });
});
