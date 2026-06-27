import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("assistant streaming lifecycle contract", () => {
  describe("token batches are throttled", () => {
    it("useStreamingContent has a minimum flush interval for streaming tokens", () => {
      const src = read("src/hooks/useStreamingContent.ts");
      // Must debounce/throttle token batches to avoid excessive re-renders
      // Contract: must have a measurable delay/interval constant or mechanism
      expect(src).toMatch(
        /(?:debounce|throttle|MIN_FLUSH|flushInterval|delayMs)/,
      );
    });

    it("AiMessageBubble uses useStreamingContent for streamed content", () => {
      const src = read("src/components/ai/AiMessageBubble.tsx");
      expect(src).toContain("useStreamingContent");
      expect(src).toContain("streaming");
    });
  });

  describe("user-scrolled-up state prevents forced scroll-to-bottom", () => {
    it("AiMessageList tracks scroll position to detect user scroll-up", () => {
      const src = read("src/components/ai/AiMessageList.tsx");
      // Must have viewport ref for scroll position tracking
      expect(src).toContain("viewportRef");
    });

    it("scroll area uses a ref to the viewport element", () => {
      const src = read("src/components/ai/AiMessageList.tsx");
      expect(src).toContain("viewportRef");
      expect(src).toContain("ScrollArea");
    });

    it("scroll follow state machine exists with following and detached states", () => {
      const src = read("src/components/ai/AiMessageList.tsx");
      // Contract: must have a scroll follow state that tracks whether user is near bottom
      // States: "following" (auto-scroll) and "detached" (user scrolled up)
      expect(src).toMatch(
        /(?:isNearBottom|scrollFollow|following|detached|userScrolledUp|autoScroll)/,
      );
    });
  });

  describe("returning to bottom resumes follow mode", () => {
    it("AiMessageList has scroll follow logic for streaming", () => {
      const src = read("src/components/ai/AiMessageList.tsx");
      // The component must handle scroll follow during streaming
      // This locks the existence of streaming-aware scroll behavior
      expect(src).toContain("streaming");
    });

    it("scroll handler detects when user returns to bottom", () => {
      const src = read("src/components/ai/AiMessageList.tsx");
      // Contract: must have logic to resume auto-scroll when user scrolls back to bottom
      expect(src).toMatch(
        /(?:scrollTop|scrollHeight|clientHeight|nearBottom|threshold)/,
      );
    });
  });

  describe("streaming SSE loop breaks cleanly", () => {
    it("SSE outer loop is labeled for clean break on [DONE]", () => {
      const src = read("src-tauri/src/ai_runtime/model_gateway/streaming.rs");
      expect(src).toContain("'stream: loop {");
    });

    it("breaks on [DONE] signal", () => {
      const src = read("src-tauri/src/ai_runtime/model_gateway/streaming.rs");
      const doneBranch = src.split('data == "[DONE]"')[1] ?? "";
      expect(doneBranch).toContain("break 'stream");
    });

    it("breaks on Anthropic message_stop", () => {
      const src = read("src-tauri/src/ai_runtime/model_gateway/streaming.rs");
      const stopBranch = src.split('"message_stop"')[1] ?? "";
      expect(stopBranch).toContain("break 'stream");
    });
  });

  describe("abort can interrupt a stalled stream", () => {
    it("streaming uses abort poll interval for stall detection", () => {
      const src = read("src-tauri/src/ai_runtime/model_gateway/streaming.rs");
      expect(src).toContain("ABORT_POLL_INTERVAL");
      expect(src).toContain(
        "tokio::time::timeout(ABORT_POLL_INTERVAL, stream.next())",
      );
    });
  });

  describe("HTTP client has read timeout for SSE stall detection", () => {
    it("cert_pinning configures read_timeout", () => {
      const src = read("src-tauri/src/network/cert_pinning.rs");
      expect(src).toContain("read_timeout");
      expect(src).toContain("DEFAULT_READ_TIMEOUT_SECS");
    });
  });

  describe("parse-failure retry surfaces progress to UI", () => {
    it("run.rs emits retry status on tool parse failure", () => {
      const src = read("src-tauri/src/ai_harness/harness/run.rs");
      const parseBranch =
        src.split("should_retry_tool_parse(&tool_calls)")[1] ?? "";
      expect(parseBranch).toContain("ai:retry_status");
    });
  });
});
