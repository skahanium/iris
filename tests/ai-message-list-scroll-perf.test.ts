import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("AI message list scroll performance fixes (Fix 2 + Fix 3)", () => {
  describe("Fix 1: message identity instead of virtual index keys", () => {
    it("uses stable assistant message identity for virtualizer rows", () => {
      const s = read("src/components/ai/AiMessageList.tsx");

      expect(s).toContain("getItemKey");
      expect(s).toContain("assistantMessageIdentity");
      expect(s).not.toContain("key={virtualRow.key}");
    });
  });

  describe("Fix 2: live and completed rows share one stable parent", () => {
    it("keeps historical estimates independent from live stream height", () => {
      const s = read("src/components/ai/AiMessageList.tsx");
      expect(s).not.toContain("estimateSize: () => 112");
      expect(s).toContain("estimateRowSize");
      expect(s).toContain("conversationRows");
      expect(s).not.toContain("historicalRows");
      expect(s).not.toContain("liveRows.map");
      expect(s).toContain("data-live-stream");
      expect(s).not.toContain("? 320");
      expect(s).not.toContain("content.length *");
    });
  });

  describe("Fix 3: stable callbacks to preserve memo during streaming", () => {
    it("does not create inline arrow callbacks for onRetract/onCopy in JSX", () => {
      const s = read("src/components/ai/AiMessageList.tsx");
      // The old code had `onRetract ? () => onRetract(i) : undefined` and
      // `() => handleCopyMessage(m)` inline in JSX — new refs every render,
      // breaking AiMessageBubble's memo. These must be stabilized.
      expect(s).not.toContain("onRetract ? () => onRetract(i) : undefined");
      expect(s).not.toContain("() => handleCopyMessage(m)");
    });
  });
  describe("Fix 4: stable virtualizer measurement ref", () => {
    it("does not pass rowVirtualizer.measureElement directly as a React ref", () => {
      const s = read("src/components/ai/AiMessageList.tsx");

      expect(s).not.toContain("ref={rowVirtualizer.measureElement}");
      expect(s).toContain("measureRowElement");
    });

    it("uses TanStack row observation and delegates compensation to the anchor", () => {
      const s = read("src/components/ai/AiMessageList.tsx");
      expect(s).toContain(
        "rowVirtualizer.shouldAdjustScrollPositionOnItemSizeChange = () => false",
      );
      expect(s).toContain("geometryCallbackRef.current()");
      expect(s).not.toContain("new ResizeObserver");
    });
  });

  describe("Fix 5: reading-anchor controller", () => {
    it("delegates pre-paint scroll writes and user detach handling to the reading-anchor hook", () => {
      const s = read("src/components/ai/AiMessageList.tsx");

      expect(s).toContain("useConversationReadingAnchor");
      expect(s).toContain("returnToLatest");
      expect(s).toContain("回到最新");
    });

    it("keys follow state and scroll revisions to the live assistant message", () => {
      const s = read("src/components/ai/AiMessageList.tsx");

      expect(s).toContain("const activeStreamingMessage");
      expect(s).toContain("const activeStreamKey");
      expect(s).toContain("scheduleGeometry");
      expect(s).toContain("conversationFollowStreamKey");
      expect(s).toContain("userClientRequestId");
      expect(s).toContain("revision: conversationRows.length");
      expect(s).not.toContain("liveFooterHeight");
      expect(s).not.toContain("activeStreamingMessage?.content.length");
    });

    it("writes scrollTop at most once per follow pass and does not re-observe the tail", () => {
      const hook = read(
        "src/components/ai/hooks/useConversationReadingAnchor.ts",
      );

      expect(hook).toContain("lastScrollTop = actualScrollTop;");
      expect(hook).toContain("iris-conversation-geometry");
      expect(hook).toContain("conversationFollowTarget");
      expect(hook.match(/viewport\.scrollTop =/g)?.length).toBe(1);
      expect(hook).not.toContain("setTailRevision");
      expect(hook).not.toContain("[data-streaming-tail]");
    });

    it("reserves a bottom spacer so the latest text never touches the viewport edge", () => {
      const s = read("src/components/ai/AiMessageList.tsx");

      expect(s).toContain('className="h-24 shrink-0"');
      expect(s).toContain("data-conversation-spacer");
      expect(s).toContain("data-conversation-park");
      expect(s).toContain("aria-hidden");
    });

    it("keeps following while a message is still in streaming presentation", () => {
      const s = read("src/components/ai/AiMessageList.tsx");

      expect(s).toMatch(
        /active:\s*streaming \|\| activeStreamingMessage != null/,
      );
    });

    it("renders the return-to-latest control as a centered circular down-arrow button", () => {
      const s = read("src/components/ai/AiMessageList.tsx");

      expect(s).toContain("ArrowDown");
      expect(s).toContain("left-1/2");
      expect(s).toContain("-translate-x-1/2");
      expect(s).toContain("rounded-full");
      expect(s).toContain('aria-label="回到最新"');
      expect(s).not.toContain(">回到最新<");
    });
  });
});
