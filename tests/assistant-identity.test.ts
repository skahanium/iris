import { describe, expect, it, beforeEach } from "vitest";

import {
  assistantInitial,
  loadAssistantIdentity,
  sanitizeAvatarEmoji,
  sanitizeDisplayName,
  saveAssistantIdentity,
} from "@/lib/assistant-identity";
import {
  describeAssistantContext,
  describeAssistantSubtitle,
} from "@/lib/assistant-context-label";

describe("assistant identity", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("persists display name and emoji", () => {
    saveAssistantIdentity({ displayName: "小鸢", avatarEmoji: "🦉" });
    expect(loadAssistantIdentity()).toEqual({
      displayName: "小鸢",
      avatarEmoji: "🦉",
    });
  });

  it("sanitizes overly long names and multi-codepoint emoji input", () => {
    expect(sanitizeDisplayName("  文献助手  ")).toBe("文献助手");
    expect(sanitizeAvatarEmoji("🦉✨")).toBe("🦉");
    expect(assistantInitial("小鸢")).toBe("小");
  });
});

describe("assistant context labels", () => {
  it("uses plain language for empty editor state", () => {
    expect(describeAssistantContext({})).toBe("未打开笔记");
    expect(describeAssistantContext({ noteDisplayTitle: "民法笔记" })).toBe(
      "当前笔记：民法笔记",
    );
  });

  it("shows task hint only when busy", () => {
    expect(
      describeAssistantSubtitle({
        status: "idle",
        contextLabel: "未打开笔记",
        intentLabel: "对话",
        statusLabel: "待命",
        showTaskHint: false,
      }),
    ).toBe("未打开笔记");

    expect(
      describeAssistantSubtitle({
        status: "running",
        contextLabel: "当前笔记：Demo",
        intentLabel: "知识查阅",
        statusLabel: "处理中",
        showTaskHint: true,
      }),
    ).toBe("知识查阅 · 处理中");
  });
});
