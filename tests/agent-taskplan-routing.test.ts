import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

interface AssistantRouteInput {
  message: string;
  hasSelection: boolean;
  notePath: string | null;
  explicitScope: boolean;
}

interface IntentDetectionResult {
  detectedIntent: string;
}

interface AssistantRoutingModule {
  detectAgentIntent(input: AssistantRouteInput): IntentDetectionResult;
}

async function loadRouting(): Promise<AssistantRoutingModule> {
  const spec = "../src/lib/assistant-routing";
  return (await import(/* @vite-ignore */ spec)) as AssistantRoutingModule;
}

describe("assistant TaskPlan routing contract", () => {
  it("creates a per-turn TaskPlan instead of locking a conversation scene", () => {
    const taskplan = read("src/lib/assistant-taskplan.ts");

    expect(taskplan).toContain("buildAssistantTaskPlan");
    expect(taskplan).toContain("contextReferences");
    expect(taskplan).toContain("retrievalMode");
    expect(taskplan).toContain("executionMode");
    expect(taskplan).toContain("artifactPlan");
  });

  it("keeps novel continuation with analysis words on the writer path", () => {
    const taskplan = read("src/lib/assistant-taskplan.ts");

    expect(taskplan).toContain("creative_write");
    expect(taskplan).toContain("requiresClarification");
    expect(taskplan).toContain("writingKeywordBeforeResearchKeyword");
  });

  it("keeps legacy routing as an adapter, not the primary decision system", () => {
    const routing = read("src/lib/assistant-routing.ts");

    expect(routing).toContain("buildAssistantTaskPlan");
  });

  it("removes the legacy research keyword priority from routing", () => {
    const routing = read("src/lib/assistant-routing.ts");

    expect(routing).not.toContain("const RESEARCH_KEYWORDS");
    expect(routing).not.toContain("includesAny(message, RESEARCH_KEYWORDS)");
  });

  it("does not detect fiction continuation as research just because it says 分析 or 研究", async () => {
    const routing = await loadRouting();
    const result = routing.detectAgentIntent({
      message:
        "根据以上文字写出第四章，要求描写更火爆、剧情更诱人，同时分析人物心理",
      hasSelection: true,
      notePath: "/novel.md",
      explicitScope: false,
    });

    expect(result.detectedIntent).not.toBe("research");
    expect(["rewrite_selection", "write"]).toContain(result.detectedIntent);
  });

  it("keeps explicit research questions on the research path", async () => {
    const routing = await loadRouting();
    const result = routing.detectAgentIntent({
      message: "研究一下 sqlite-vec 和 FTS5 在本地知识库中的取舍",
      hasSelection: false,
      notePath: null,
      explicitScope: true,
    });

    expect(result.detectedIntent).toBe("research");
  });
});
