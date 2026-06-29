import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("AI message list scroll performance fixes (Fix 2 + Fix 3)", () => {
  describe("Fix 2: content-aware estimateSize", () => {
    it("estimateSize is a function of row content, not a fixed 112 constant", () => {
      const s = read("src/components/ai/AiMessageList.tsx");
      // The old `estimateSize: () => 112` is a constant that's wildly wrong
      // for tall messages. The fix must make it content-aware.
      expect(s).not.toContain("estimateSize: () => 112");
      // Must reference a content/length-based heuristic.
      expect(s).toMatch(
        /estimateSize.*content|estimateRowHeight|estimateSizeByContent/,
      );
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
  });
});
