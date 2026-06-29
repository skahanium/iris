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

    it("AiMessageBubble defers streamed markdown snapshots but renders final content immediately", () => {
      const src = read("src/components/ai/AiMessageBubble.tsx");

      expect(src).toContain("useDeferredValue");
      expect(src).toContain("const deferredRenderContent = useDeferredValue");
      expect(src).toContain(
        "const markdownContent = streaming ? deferredRenderContent : content",
      );
      expect(src).toContain("renderMarkdownWithProfile(");
      expect(src).toContain('markdownContent || ""');
    });

    it("AiMessageBubble uses the markdown render worker only for streaming assistant content", () => {
      const src = read("src/components/ai/AiMessageBubble.tsx");

      expect(src).toContain("useMarkdownRenderWorker");
      expect(src).toContain("workerRender = useMarkdownRenderWorker");
      expect(src).toContain("enabled: streaming");
      expect(src).toContain("workerRender.html");
      expect(src).toContain("workerRender.failed");
    });

    it("AiMessageList keeps the latest assistant message streaming even when it has content", () => {
      const src = read("src/components/ai/AiMessageList.tsx");
      expect(src).toContain(
        'const assistantStreaming = streaming && m.role === "assistant" && isLast',
      );
      expect(src).not.toContain(
        'streaming && m.role === "assistant" && isLast && !m.content',
      );
    });

    it("useAssistantLlmStream batches token snapshots on animation frames", () => {
      const src = read("src/hooks/useAssistantLlmStream.ts");
      expect(src).toContain("window.requestAnimationFrame");
      expect(src).toContain("window.cancelAnimationFrame");
      expect(src).not.toContain("lastFlushRef");
      expect(src).not.toContain("clearTimeout");
    });

    it("AiMessageList does not depend scroll-follow effects on the virtualizer object", () => {
      const src = read("src/components/ai/AiMessageList.tsx");
      expect(src).toContain("const virtualTotalSize =");
      expect(src).toContain("const virtualItems =");
      expect(src).not.toContain(
        "[messages, rows.length, rowVirtualizer, scrollFollow, streaming]",
      );
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

  describe("classified chat uses the unified streaming lifecycle", () => {
    it("classified chat emits request_started before model work and does not force stream false", () => {
      const src = read("src-tauri/src/ai_harness/harness_task.rs");
      const fnBody =
        src
          .split("async fn run_classified_chat_task")[1]
          ?.split("async fn")[0] ?? "";

      expect(fnBody).toContain('"ai:request_started"');
      expect(fnBody).toContain("send_classified_streaming_request");
      expect(fnBody).not.toContain("stream: false");
    });

    it("stream events can mark classified payloads for domain filtering", () => {
      const streaming = read(
        "src-tauri/src/ai_runtime/model_gateway/streaming.rs",
      );
      const ipcTypes = read("src/types/ipc.ts");

      expect(ipcTypes).toContain("classified?: boolean");
      expect(streaming).toContain("classified");
      expect(streaming).toContain("send_streaming_request_with_meta");
      expect(streaming).toContain('"classified"');
    });
  });
  describe("request identity is available before IPC completion", () => {
    it("ai_send_message emits request_started after durable session and task exist", () => {
      const src = read("src-tauri/src/commands/ai_commands.rs");
      const fnBody =
        src
          .split("pub(crate) async fn execute_ai_send_message_with_routing")[1]
          ?.split("info!(")[0] ?? "";
      const createTaskIndex = fnBody.indexOf("AgentTaskRuntime::create_task");
      const emitIndex = fnBody.indexOf('"ai:request_started"');

      expect(createTaskIndex).toBeGreaterThanOrEqual(0);
      expect(emitIndex).toBeGreaterThan(createTaskIndex);
      expect(fnBody).toContain('"session_id": sid');
      expect(fnBody).toContain('"task_id": task_id');
    });

    it("request_started listener stays mounted outside streaming windows", () => {
      const src = read("src/components/ai/hooks/useAssistantPanelEffects.ts");
      const effectBody = src.split("listenAiRequestStarted")[1] ?? "";

      expect(effectBody).not.toContain("if (!streaming) return");
      expect(effectBody).toContain("setHarnessRequestId");
      expect(effectBody).toContain("setAgentTaskId");
      expect(effectBody).toContain("setSessionId");
    });
  });
  describe("composer abort targets durable tasks first", () => {
    it("stopStreaming aborts by agent task before falling back to harness request", () => {
      const src = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");
      const fnBody =
        src
          .split("const stopStreaming = useCallback")[1]
          ?.split("const togglePacketSelection")[0] ?? "";

      expect(fnBody).toContain("agentTaskAbort");
      expect(fnBody).toContain("agentTaskId");
      expect(fnBody).toContain("harnessAbort");
      expect(fnBody.indexOf("agentTaskAbort")).toBeLessThan(
        fnBody.indexOf("harnessAbort"),
      );
    });
  });
});
