import { describe, expect, it, beforeEach } from "vitest";

import {
  describeAssistantContext,
  describeAssistantSubtitle,
} from "@/lib/assistant-context-label";
import {
  AVATAR_IDS,
  DEFAULT_AVATAR_ID,
  DEFAULT_DISPLAY_NAME,
  DEFAULT_PROMPT_PROFILE,
  clearPersistedLegacyAssistantIdentity,
  mergeLegacyAssistantIdentity,
  normalizeAvatarId,
  normalizePromptProfile,
  profileToAvatarIdentity,
  sanitizeDisplayName,
} from "@/lib/prompt-profile";

describe("prompt profile", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("normalizes display name and accepts only the built-in geometric avatar ids", () => {
    expect(sanitizeDisplayName("  文献助手  ")).toBe("文献助手");
    expect(AVATAR_IDS).toEqual([
      "iris",
      "orbit",
      "axis",
      "frame",
      "lens",
      "grid",
      "flow",
      "signal",
    ]);
    expect(normalizeAvatarId("orbit")).toBe("orbit");
    expect(normalizeAvatarId("🦉")).toBe(DEFAULT_AVATAR_ID);
    expect(normalizeAvatarId("not-an-avatar")).toBe(DEFAULT_AVATAR_ID);
  });

  it("maps profile to avatar identity", () => {
    expect(
      profileToAvatarIdentity({
        ...DEFAULT_PROMPT_PROFILE,
        display_name: "小鸢",
        avatar_id: "lens",
      }),
    ).toEqual({
      displayName: "小鸢",
      avatarId: "lens",
    });
  });

  it("keeps legacy localStorage identity until SQLite persistence succeeds", () => {
    localStorage.setItem(
      "iris-assistant-identity",
      JSON.stringify({ displayName: "Iris", avatarEmoji: "✨" }),
    );
    const { profile, migrated } = mergeLegacyAssistantIdentity(
      DEFAULT_PROMPT_PROFILE,
    );
    expect(migrated).toBe(true);
    expect(profile.display_name).toBe("Iris");
    expect(profile.avatar_id).toBe(DEFAULT_AVATAR_ID);
    expect(localStorage.getItem("iris-assistant-identity")).not.toBeNull();

    clearPersistedLegacyAssistantIdentity();
    expect(localStorage.getItem("iris-assistant-identity")).toBeNull();
  });

  it("falls back to default display name when empty", () => {
    const profile = normalizePromptProfile({
      display_name: "   ",
      avatar_id: "signal",
      persona: "",
      writing_style: "",
      custom_rules: [],
      language: "zh-CN",
    });
    expect(profile.display_name).toBe(DEFAULT_DISPLAY_NAME);
    expect(profile.avatar_id).toBe("signal");
  });

  it("falls back to the Iris mark when a legacy emoji profile is loaded", () => {
    const profile = normalizePromptProfile({
      display_name: "砚",
      avatar_emoji: "🖋️",
    } as never);

    expect(profile.avatar_id).toBe(DEFAULT_AVATAR_ID);
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
