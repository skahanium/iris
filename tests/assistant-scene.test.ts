import { describe, expect, it } from "vitest";

import {
  legacySceneHintForAgentIntent,
  legacySceneHintForAssistantIntent,
} from "@/lib/assistant-scene";

describe("legacySceneHintForAssistantIntent", () => {
  it("maps drafting intents to drafting_assist", () => {
    expect(legacySceneHintForAssistantIntent("writing")).toBe(
      "drafting_assist",
    );
    expect(legacySceneHintForAssistantIntent("chapter")).toBe(
      "drafting_assist",
    );
    expect(legacySceneHintForAssistantIntent("document")).toBe(
      "drafting_assist",
    );
  });

  it("maps research to research_synthesis", () => {
    expect(legacySceneHintForAssistantIntent("research")).toBe(
      "research_synthesis",
    );
  });

  it("maps knowledge and chat to knowledge_lookup", () => {
    expect(legacySceneHintForAssistantIntent("knowledge")).toBe(
      "knowledge_lookup",
    );
    expect(legacySceneHintForAssistantIntent("chat")).toBe("knowledge_lookup");
  });
});

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
});
