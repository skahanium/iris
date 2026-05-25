import { describe, expect, it } from "vitest";

import { buildInlineAiUserMessage } from "@/lib/inline-ai-prompts";
import {
  buildSlashCommandMessage,
  parseSlashActionId,
  slashActionId,
} from "@/lib/slash-command-prompts";

describe("inline-ai-prompts", () => {
  it("builds rewrite message", () => {
    const msg = buildInlineAiUserMessage("rewrite", "hello world");
    expect(msg).toContain("改写");
    expect(msg).toContain("hello world");
  });

  it("builds translate message", () => {
    const msg = buildInlineAiUserMessage("translate", "你好");
    expect(msg).toContain("翻译");
    expect(msg).toContain("你好");
  });

  it("falls back for unknown action", () => {
    const msg = buildInlineAiUserMessage("unknown_action", "text");
    expect(msg).toContain("请处理以下文字");
    expect(msg).toContain("text");
  });
});

describe("slash-command-prompts", () => {
  it("builds summarize command", () => {
    const msg = buildSlashCommandMessage("summarize");
    expect(msg).toContain("总结");
  });

  it("falls back to raw command for unknown", () => {
    const msg = buildSlashCommandMessage("unknown-cmd");
    expect(msg).toBe("unknown-cmd");
  });

  it("slashActionId prefixes correctly", () => {
    expect(slashActionId("fix-grammar")).toBe("slash:fix-grammar");
  });

  it("parseSlashActionId extracts command", () => {
    expect(parseSlashActionId("slash:outline")).toBe("outline");
  });

  it("parseSlashActionId returns null for non-slash", () => {
    expect(parseSlashActionId("rewrite")).toBeNull();
  });
});
