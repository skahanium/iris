import { describe, expect, it } from "vitest";

import type { ChatLine } from "@/components/ai/AiMessageList";
import {
  assistantChromeSnapshotsEqual,
  buildAssistantChromeSnapshot,
  resolveToolActivityLabel,
} from "@/lib/assistant-chrome";
import { TOOL_DISPLAY_NAMES, toolDisplayName } from "@/lib/tool-display-names";

describe("assistant chrome helpers", () => {
  it("counts only safe EvidenceRef metadata", () => {
    const snapshot = buildAssistantChromeSnapshot({
      sessionTokenUsage: {
        prompt_tokens: 1,
        completion_tokens: 2,
        total_tokens: 3,
      },
      evidence: [
        {
          evidenceId: "local-1",
          sourceKind: "local",
          displayLabel: "[1]",
          stale: false,
        },
        {
          evidenceId: "web-1",
          sourceKind: "web",
          displayLabel: "[2]",
          stale: false,
        },
      ],
    });

    expect(snapshot.evidenceCount).toBe(2);
    expect(snapshot.webEvidenceCount).toBe(1);
    expect(snapshot.toolActivityLabel).toBeNull();
  });

  it("forwards activityHint into toolActivityLabel", () => {
    const snapshot = buildAssistantChromeSnapshot({
      sessionTokenUsage: null,
      evidence: [],
      activityHint: "正在联网搜索…",
      streaming: true,
      messages: [],
      harnessPhaseLabel: null,
    });
    expect(snapshot.toolActivityLabel).toBe("正在联网搜索…");
  });

  it("uses the pending tool display name when no stage hint exists", () => {
    const messages: ChatLine[] = [
      {
        role: "assistant",
        content: "",
        toolCalls: [{ id: "1", name: "web_search", status: "pending" }],
      },
    ];
    expect(
      resolveToolActivityLabel({
        activityHint: null,
        streaming: true,
        messages,
        harnessPhaseLabel: null,
      }),
    ).toBe("联网搜索");
  });

  it("compares visible snapshot fields by value", () => {
    const left = buildAssistantChromeSnapshot({
      sessionTokenUsage: null,
      evidence: [],
    });
    const same = { ...left };
    const changed = { ...left, evidenceCount: 1 };

    expect(assistantChromeSnapshotsEqual(left, same)).toBe(true);
    expect(assistantChromeSnapshotsEqual(left, changed)).toBe(false);
  });

  it("uses Chinese names for current read tools and a generic Chinese fallback", () => {
    expect(toolDisplayName("web_fetch")).toBe("读取网页");
    expect(toolDisplayName("read_note")).toBe("读取笔记");
    expect(toolDisplayName("get_outline")).toBe("文档大纲");
    expect(toolDisplayName("get_backlinks")).toBe("反向链接");
    expect(TOOL_DISPLAY_NAMES).not.toHaveProperty("get_block_links");
    expect(toolDisplayName("get_block_links")).toBe("执行工具");
  });
});
