import { describe, expect, it } from "vitest";

import { sanitizeAssistantVisibleText } from "@/lib/assistant-visible-text";

describe("sanitizeAssistantVisibleText", () => {
  it("剥离完整与半开的 reasoning 标签", () => {
    expect(
      sanitizeAssistantVisibleText("<thinking>内部计划</thinking>可见答案"),
    ).toBe("可见答案");
    expect(sanitizeAssistantVisibleText("前言<think")).toBe("前言");
  });
});
