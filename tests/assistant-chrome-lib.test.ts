import { describe, expect, it } from "vitest";

import type { ChatLine } from "@/components/ai/AiMessageList";
import {
  assistantChromeSnapshotsEqual,
  buildAssistantChromeSnapshot,
  countWebPageFetchPackets,
  countWebSearchPackets,
  resolveToolActivityLabel,
} from "@/lib/assistant-chrome";

describe("assistant chrome helpers", () => {
  it("buildAssistantChromeSnapshot keeps process hints out of the bottom status bar", () => {
    const snap = buildAssistantChromeSnapshot({
      sessionTokenUsage: null,
      activityHint: "正在检索知识库与本地笔记…",
      streaming: true,
      messages: [],
      harnessPhaseLabel: null,
      packets: [],
    });
    expect(snap.toolActivityLabel).toBeNull();
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
    expect(snap.harnessRequestId).toBeNull();
  });

  it("buildAssistantChromeSnapshot forwards harness request id", () => {
    const snap = buildAssistantChromeSnapshot({
      sessionTokenUsage: null,
      activityHint: null,
      streaming: true,
      messages: [],
      harnessPhaseLabel: null,
      packets: [],
      harnessRequestId: "req-1",
    });
    expect(snap.harnessRequestId).toBe("req-1");
  });

  it("assistantChromeSnapshotsEqual ignores object identity but detects visible fields", () => {
    const left = buildAssistantChromeSnapshot({
      sessionTokenUsage: {
        prompt_tokens: 1,
        completion_tokens: 2,
        total_tokens: 3,
      },
      activityHint: "正在搜索",
      streaming: true,
      messages: [],
      harnessPhaseLabel: null,
      packets: [],
      harnessRequestId: "req-1",
    });
    const same = {
      ...left,
      sessionTokenUsage: { ...left.sessionTokenUsage! },
    };
    const changed = { ...left, evidenceCount: left.evidenceCount + 1 };

    expect(assistantChromeSnapshotsEqual(left, same)).toBe(true);
    expect(assistantChromeSnapshotsEqual(left, changed)).toBe(false);
  });

  it("splits web search vs page fetch packets", () => {
    const packets = [
      {
        id: "w1",
        source_type: "web" as const,
        source_path: "https://a.com",
        title: "A",
        heading_path: null,
        source_span: null,
        content_hash: "h",
        excerpt: "snippet",
        retrieval_reason: "web_search",
        score: 1,
        trust_level: "external_web" as const,
        citation_label: "[W0]",
        stale: false,
      },
      {
        id: "w2",
        source_type: "web" as const,
        source_path: "https://b.com",
        title: "B",
        heading_path: null,
        source_span: null,
        content_hash: "h2",
        excerpt: "body",
        retrieval_reason: "web_page_fetch",
        score: 1,
        trust_level: "external_web" as const,
        citation_label: "[Wp]",
        stale: false,
      },
    ];
    expect(countWebSearchPackets(packets)).toBe(1);
    expect(countWebPageFetchPackets(packets)).toBe(1);
  });

  it("resolveToolActivityLabel shows web_search name", () => {
    const messages: ChatLine[] = [
      {
        role: "assistant",
        content: "",
        toolCalls: [
          {
            id: "2",
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
});
