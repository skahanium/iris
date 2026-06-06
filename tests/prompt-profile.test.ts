import { describe, expect, it, beforeEach } from "vitest";

import {
  describeAssistantContext,
  describeAssistantSubtitle,
} from "@/lib/assistant-context-label";
import {
  DEFAULT_DISPLAY_NAME,
  DEFAULT_PROMPT_PROFILE,
  mergeLegacyAssistantIdentity,
  normalizePromptProfile,
  profileToAvatarIdentity,
  sanitizeAvatarEmoji,
  sanitizeDisplayName,
  assistantInitial,
} from "@/lib/prompt-profile";

describe("prompt profile", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("normalizes display name and avatar emoji", () => {
    expect(sanitizeDisplayName("  文献助手  ")).toBe("文献助手");
    expect(sanitizeAvatarEmoji("🦉✨")).toBe("🦉");
    expect(assistantInitial("小鸢")).toBe("小");
  });

  it("maps profile to avatar identity", () => {
    expect(
      profileToAvatarIdentity({
        ...DEFAULT_PROMPT_PROFILE,
        display_name: "小鸢",
        avatar_emoji: "🦉",
      }),
    ).toEqual({
      displayName: "小鸢",
      avatarEmoji: "🦉",
    });
  });

  it("migrates legacy localStorage identity into default profile", () => {
    localStorage.setItem(
      "iris-assistant-identity",
      JSON.stringify({ displayName: "Iris", avatarEmoji: "✨" }),
    );
    const { profile, migrated } = mergeLegacyAssistantIdentity(
      DEFAULT_PROMPT_PROFILE,
    );
    expect(migrated).toBe(true);
    expect(profile.display_name).toBe("Iris");
    expect(profile.avatar_emoji).toBe("✨");
    expect(localStorage.getItem("iris-assistant-identity")).toBeNull();
  });

  it("falls back to default display name when empty", () => {
    const profile = normalizePromptProfile({
      display_name: "   ",
      avatar_emoji: null,
      persona: "",
      writing_style: "",
      custom_rules: [],
      language: "zh-CN",
    });
    expect(profile.display_name).toBe(DEFAULT_DISPLAY_NAME);
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
