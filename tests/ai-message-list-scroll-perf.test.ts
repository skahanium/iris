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

  describe("Fix 2: stable streaming estimate", () => {
    it("keeps streaming height independent from content length until ResizeObserver measures it", () => {
      const s = read("src/components/ai/AiMessageList.tsx");
      expect(s).not.toContain("estimateSize: () => 112");
      expect(s).toContain("estimateRowSize");
      expect(s).toContain("? 320");
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

    it("batches virtualizer row measurements on animation frames", () => {
      const s = read("src/components/ai/AiMessageList.tsx");
      const scheduleCallback = s.split("const scheduleMeasureFrame")[1] ?? "";
      const rowCallback = s.split("const measureRowElement")[1] ?? "";

      expect(scheduleCallback).toContain("requestAnimationFrame");
      expect(scheduleCallback).toContain("cancelAnimationFrame");
      expect(scheduleCallback).toContain("pendingMeasureNodesRef");
      expect(rowCallback).toContain("ResizeObserver");
      expect(rowCallback).toContain("scheduleMeasureFrame");
      expect(scheduleCallback).not.toContain(
        "rowVirtualizerRef.current.measureElement(node)",
      );
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
      expect(s).toContain("const contentRevision");
      expect(s).toContain("streamKey: activeStreamKey");
      expect(s).toContain("revision: contentRevision");
    });

    it("keeps the tail observer stable for one streaming message", () => {
      const hook = read(
        "src/components/ai/hooks/useConversationReadingAnchor.ts",
      );

      expect(hook).toContain("}, [active, streamKey, viewportRef]);");
      expect(hook).toContain("lastObservedScrollTopRef.current = target;");
    });

    it("reserves a bottom spacer so the latest text never touches the viewport edge", () => {
      const s = read("src/components/ai/AiMessageList.tsx");

      expect(s).toContain('className="h-24 shrink-0"');
      expect(s).toContain("aria-hidden");
    });

    it("keeps following while a message is still in streaming presentation", () => {
      const s = read("src/components/ai/AiMessageList.tsx");

      expect(s).toContain(
        "active: streaming || activeStreamingMessage != null",
      );
    });
  });
});
