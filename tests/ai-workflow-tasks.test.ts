import { describe, expect, it } from "vitest";

import { isPlaceholderTitle } from "@/lib/path-sync";
import type {
  AssistantActionState,
  AssistantIntent,
  AssistantSurfaceState,
  AssistantTaskStatus,
} from "@/types/ai";

describe("note workflow helpers", () => {
  it("detects placeholder titles", () => {
    expect(isPlaceholderTitle("")).toBe(false);
    expect(isPlaceholderTitle("未命名文档")).toBe(true);
    expect(isPlaceholderTitle("新建文档")).toBe(true);
    expect(isPlaceholderTitle("untitled-1")).toBe(true);
    expect(isPlaceholderTitle("民法笔记")).toBe(false);
  });
});

describe("assistant state types", () => {
  it("supports unified assistant intents and surface states", () => {
    const intents: AssistantIntent[] = [
      "chat",
      "knowledge",
      "writing",
      "citation",
      "organize",
      "research",
      "chapter",
      "document",
    ];
    const statuses: AssistantTaskStatus[] = [
      "idle",
      "running",
      "awaiting_confirmation",
      "completed",
      "error",
    ];
    const surfaceStates: AssistantSurfaceState[] = [
      "conversation",
      "inline_suggestion",
      "diff_review",
      "research_focus",
    ];
    const action: AssistantActionState = {
      intent: "knowledge",
      status: "idle",
      label: "知识查阅",
    };

    expect(intents).toHaveLength(8);
    expect(statuses).toContain("awaiting_confirmation");
    expect(surfaceStates).toContain("research_focus");
    expect(action.intent).toBe("knowledge");
  });
});
