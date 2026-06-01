import { describe, expect, it } from "vitest";

import type { ChatLine } from "@/components/ai/AiMessageList";
import {
  buildAssistantChromeSnapshot,
  resolveToolActivityLabel,
} from "@/lib/assistant-chrome";

describe("assistant chrome helpers", () => {
  it("prefers activityHint over tool name when streaming", () => {
    const label = resolveToolActivityLabel({
      activityHint: "正在检索知识库与本地笔记…",
      streaming: true,
      messages: [],
      harnessPhaseLabel: null,
    });
    expect(label).toBe("正在检索知识库与本地笔记…");
  });

  it("uses pending tool display name when no hint", () => {
    const messages: ChatLine[] = [
      {
        role: "assistant",
        content: "",
        toolCalls: [
          {
            id: "1",
            name: "web_search",
            status: "pending",
          },
        ],
      },
    ];
    const label = resolveToolActivityLabel({
      activityHint: null,
      streaming: true,
      messages,
      harnessPhaseLabel: null,
    });
    expect(label).toBe("联网搜索");
  });

  it("buildAssistantChromeSnapshot counts web packets", () => {
    const snap = buildAssistantChromeSnapshot({
      sessionTokenUsage: {
        prompt_tokens: 1,
        completion_tokens: 2,
        total_tokens: 3,
      },
      activityHint: null,
      streaming: false,
      messages: [],
      harnessPhaseLabel: null,
      packets: [
        {
          id: "a",
          source_type: "note",
          source_path: "a.md",
          title: "A",
          heading_path: null,
          source_span: null,
          content_hash: "h",
          excerpt: "e",
          retrieval_reason: "r",
          score: 1,
          trust_level: "user_note",
          citation_label: "[1]",
          stale: false,
        },
        {
          id: "b",
          source_type: "web",
          source_path: null,
          title: "B",
          heading_path: null,
          source_span: null,
          content_hash: "h2",
          excerpt: "e2",
          retrieval_reason: "r",
          score: 1,
          trust_level: "external_web",
          citation_label: "[W0]",
          stale: false,
        },
      ],
    });
    expect(snap.evidenceCount).toBe(2);
    expect(snap.webPacketCount).toBe(1);
  });
});
