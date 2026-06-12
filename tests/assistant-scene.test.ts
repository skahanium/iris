import { describe, expect, it } from "vitest";

import {
  resolveAiSceneForAgentIntent,
  resolveAiSceneForIntent,
} from "@/lib/assistant-scene";

describe("resolveAiSceneForIntent", () => {
  it("maps drafting intents to drafting_assist", () => {
    expect(resolveAiSceneForIntent("writing")).toBe("drafting_assist");
    expect(resolveAiSceneForIntent("chapter")).toBe("drafting_assist");
    expect(resolveAiSceneForIntent("document")).toBe("drafting_assist");
  });

  it("maps research to research_synthesis", () => {
    expect(resolveAiSceneForIntent("research")).toBe("research_synthesis");
  });

  it("maps knowledge and chat to knowledge_lookup", () => {
    expect(resolveAiSceneForIntent("knowledge")).toBe("knowledge_lookup");
    expect(resolveAiSceneForIntent("chat")).toBe("knowledge_lookup");
  });
});

describe("resolveAiSceneForAgentIntent", () => {
  it("maps Phase2 writing intents to drafting_assist", () => {
    expect(resolveAiSceneForAgentIntent("rewrite_selection")).toBe(
      "drafting_assist",
    );
    expect(resolveAiSceneForAgentIntent("write")).toBe("drafting_assist");
    expect(resolveAiSceneForAgentIntent("chapter")).toBe("drafting_assist");
    expect(resolveAiSceneForAgentIntent("document_check")).toBe(
      "drafting_assist",
    );
  });

  it("keeps research and note lookup on their legacy workflow scenes", () => {
    expect(resolveAiSceneForAgentIntent("research")).toBe("research_synthesis");
    expect(resolveAiSceneForAgentIntent("citation_check")).toBe(
      "research_synthesis",
    );
    expect(resolveAiSceneForAgentIntent("ask_notes")).toBe("knowledge_lookup");
    expect(resolveAiSceneForAgentIntent("organize")).toBe("knowledge_lookup");
    expect(resolveAiSceneForAgentIntent("chat")).toBe("knowledge_lookup");
  });

  it("falls back vision and skill management to safe compatible scenes", () => {
    expect(resolveAiSceneForAgentIntent("vision_chat")).toBe(
      "knowledge_lookup",
    );
    expect(resolveAiSceneForAgentIntent("skill_management")).toBe(
      "knowledge_lookup",
    );
  });
});
