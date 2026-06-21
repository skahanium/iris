import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import {
  legacySceneHintForAgentIntent,
  legacySceneHintForTaskPlanIntent,
} from "@/lib/assistant-scene";

describe("legacySceneHintForAgentIntent", () => {
  it("maps Phase2 writing intents to drafting_assist", () => {
    expect(legacySceneHintForAgentIntent("rewrite_selection")).toBe(
      "drafting_assist",
    );
    expect(legacySceneHintForAgentIntent("write")).toBe("drafting_assist");
    expect(legacySceneHintForAgentIntent("chapter")).toBe("drafting_assist");
    expect(legacySceneHintForAgentIntent("document_check")).toBe(
      "drafting_assist",
    );
  });

  it("keeps research and note lookup on their legacy workflow scenes", () => {
    expect(legacySceneHintForAgentIntent("research")).toBe(
      "research_synthesis",
    );
    expect(legacySceneHintForAgentIntent("citation_check")).toBe(
      "research_synthesis",
    );
    expect(legacySceneHintForAgentIntent("ask_notes")).toBe("knowledge_lookup");
    expect(legacySceneHintForAgentIntent("organize")).toBe("knowledge_lookup");
    expect(legacySceneHintForAgentIntent("chat")).toBe("knowledge_lookup");
  });

  it("falls back vision and skill management to safe compatible scenes", () => {
    expect(legacySceneHintForAgentIntent("vision_chat")).toBe(
      "knowledge_lookup",
    );
    expect(legacySceneHintForAgentIntent("skill_management")).toBe(
      "knowledge_lookup",
    );
  });

  it("marks the remaining scene mapping as backend compatibility only", () => {
    const source = readFileSync("src/lib/assistant-scene.ts", "utf8");
    expect(source).toContain("compatibility only");
    expect(source).not.toContain("legacySceneHintForAssistantIntent");
    expect(source).not.toContain("syncActiveLegacySceneHint");
  });

  it("maps TaskPlan intents to the session scene buckets used by history", () => {
    expect(legacySceneHintForTaskPlanIntent("citation_check")).toBe(
      "research_synthesis",
    );
    expect(legacySceneHintForTaskPlanIntent("research")).toBe(
      "research_synthesis",
    );
    expect(legacySceneHintForTaskPlanIntent("creative_write")).toBe(
      "drafting_assist",
    );
    expect(legacySceneHintForTaskPlanIntent("rewrite_selection")).toBe(
      "drafting_assist",
    );
    expect(legacySceneHintForTaskPlanIntent("document_check")).toBe(
      "drafting_assist",
    );
    expect(legacySceneHintForTaskPlanIntent("chapter")).toBe("drafting_assist");
    expect(legacySceneHintForTaskPlanIntent("ask_notes")).toBe(
      "knowledge_lookup",
    );
  });

  it("passes the TaskPlan-derived scene into assistant header history controls", () => {
    const source = readFileSync(
      "src/components/ai/UnifiedAssistantPanel.impl.tsx",
      "utf8",
    );
    expect(source).toContain("legacySceneHintForTaskPlanIntent(");
    expect(source).toContain("scene={currentScene}");
    expect(source).not.toContain('scene="knowledge_lookup"');
  });
});
