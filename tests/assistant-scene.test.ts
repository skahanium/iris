import { describe, expect, it } from "vitest";

import { resolveAiSceneForIntent } from "@/lib/assistant-scene";

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
