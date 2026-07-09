import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("AI hang/stuck root-cause fixes contract", () => {
  describe("Fix 1: streaming loop breaks on [DONE] / message_stop", () => {
    it("the SSE outer loop is labeled so it can be broken out of", () => {
      const s = read("src-tauri/src/ai_runtime/model_gateway/streaming.rs");
      // A labeled outer loop is required so [DONE]/message_stop can break
      // the stream loop instead of `continue`-ing back to wait for
      // more chunks (which hangs until the read_timeout on keep-alive sockets).
      expect(s).toContain("'stream: loop {");
    });

    it("breaks the outer stream loop on [DONE] instead of continuing", () => {
      const s = read("src-tauri/src/ai_runtime/model_gateway/streaming.rs");
      const doneBranch = s.split('data == "[DONE]"')[1] ?? "";
      // Must break the labeled outer loop, not just `continue` the inner one.
      expect(doneBranch).toContain("break 'stream");
    });

    it("breaks the outer stream loop on Anthropic message_stop", () => {
      const s = read("src-tauri/src/ai_runtime/model_gateway/streaming.rs");
      const stopBranch = s.split('"message_stop"')[1] ?? "";
      expect(stopBranch).toContain("break 'stream");
    });
  });

  describe("Fix 2: HTTP client uses read_timeout for SSE-stall detection", () => {
    it("cert_pinning configures read_timeout (per-read stall detection)", () => {
      const s = read("src-tauri/src/network/cert_pinning.rs");
      expect(s).toContain("read_timeout");
    });
  });

  describe("Fix 3: parse-failure retry surfaces progress to the UI", () => {
    it("run.rs emits ai:retry_status on tool-parse failure", () => {
      const s = read("src-tauri/src/ai_harness/harness/run.rs");
      const parseBranch =
        s.split("should_retry_tool_parse(&tool_calls)")[1] ?? "";
      expect(parseBranch).toContain("ai:retry_status");
    });
  });

  describe("Fix 4: ordinary streaming abort stays in the composer", () => {
    it("assistant composer dock does not mount a footer status strip", () => {
      const s = read("src/components/ai/AssistantComposerDock.tsx");

      expect(s).not.toContain("AssistantProcessStatusBar");
      expect(s).not.toContain("assistant-process-status");
      expect(s).not.toContain("activityHint=");
      expect(s).not.toContain("onAbort=");
    });

    it("AiComposer keeps the streaming stop button", () => {
      const s = read("src/components/ui/ai-composer.tsx");
      expect(s).toContain("streaming && onStop");
      expect(s).toContain('aria-label="停止生成"');
      expect(s).toContain("onClick={onStop}");
    });

    it("assistant task runner treats request aborted as cancellation", () => {
      const s = read("src/components/ai/hooks/useAssistantTasks.ts");
      expect(s).toContain("isAbortErrorMessage");
      expect(s).toContain('includes("request aborted")');
      expect(s).toContain('buildActionState(intent, "idle")');
    });
  });

  describe("Fix 6: run_harness enforces idle/stall timeout and abort polling", () => {
    it("does NOT use a fixed global wall-clock deadline on the streaming path", () => {
      const s = read("src-tauri/src/ai_harness/harness/run.rs");
      // The old design wrapped the entire harness in a single
      // `tokio::time::timeout(Duration::from_secs(HARNESS_DEADLINE_SECS), …)`.
      // That must be replaced with per-round idle/stall detection so a
      // slow-but-active conversation is never killed by a stale timer.
      expect(s).not.toContain("HARNESS_DEADLINE_SECS");
    });

    it("uses an idle/stall timeout mechanism instead of a global deadline", () => {
      const s = read("src-tauri/src/ai_harness/harness/run.rs");
      // The new design must detect when the harness is *idle* (no chunks
      // arriving, no tool activity) rather than measuring total wall-clock
      // elapsed time.  Accept either an explicit idle-timeout constant or
      // a per-round `tokio::time::timeout` inside the streaming/tool loop.
      const hasIdleConstant =
        s.includes("IDLE_TIMEOUT") ||
        s.includes("STALL_TIMEOUT") ||
        s.includes("HARNESS_IDLE_SECS");
      const hasPerRoundTimeout =
        s.includes("idle_timeout") || s.includes("stall_timeout");
      expect(hasIdleConstant || hasPerRoundTimeout).toBe(true);
    });

    it("polls for abort signals during idle periods via ABORT_POLL_INTERVAL", () => {
      const s = read("src-tauri/src/ai_harness/harness/run.rs");
      // When no chunks are arriving the harness must still check whether
      // the user pressed "Stop".  This is done by racing stream.next()
      // against a short abort-poll interval so the abort flag is evaluated
      // even on a half-open / stalled socket.
      expect(s).toContain("ABORT_POLL_INTERVAL");
    });
  });

  describe("Fix 7: abort can interrupt a stalled stream", () => {
    it("send_streaming_request races the stream against an abort poll", () => {
      const s = read("src-tauri/src/ai_runtime/model_gateway/streaming.rs");
      // A timeout race around stream.next() so abort is checked periodically
      // even when no chunks are arriving (stalled/half-open socket). Without
      // this, the per-chunk abort check never runs on a hung stream.
      expect(s).toContain(
        "tokio::time::timeout(ABORT_POLL_INTERVAL, stream.next())",
      );
      expect(s).toContain("ABORT_POLL_INTERVAL");
    });
  });

  describe("Fix 8: retryable stream read errors do not kill the frontend stream", () => {
    it("streaming requests ask providers for uncompressed SSE", () => {
      const s = read("src-tauri/src/ai_runtime/model_gateway/streaming.rs");

      expect(s).toContain("ACCEPT");
      expect(s).toContain("text/event-stream");
      expect(s).toContain("ACCEPT_ENCODING");
      expect(s).toContain("identity");
    });

    it("harness retries suppress non-final llm:error events", () => {
      const s = read("src-tauri/src/ai_harness/harness/run.rs");
      const retryBlock =
        s.split("send_llm_streaming_request_with_retry")[1] ?? "";

      expect(retryBlock).toContain("emit_error_event");
      expect(retryBlock).toContain("attempt == LLM_MAX_RETRIES");
    });

    it("visible partial stream errors are terminal instead of retried", () => {
      const streaming = read(
        "src-tauri/src/ai_runtime/model_gateway/streaming.rs",
      );
      const run = read("src-tauri/src/ai_harness/harness/run.rs");

      expect(streaming).toContain("partial_visible_stream_error");
      expect(streaming).toContain("surface.is_visible() && token_index > 0");
      expect(run).toContain('msg.contains("partial_visible_stream_error")');
    });

    it("llm:error payloads carry final=true only for terminal failures", () => {
      const s = read("src-tauri/src/ai_runtime/model_gateway/streaming.rs");

      expect(s).toContain("stream_error_event(");
      expect(s).toContain("final_error");
      expect(s).toContain('"final": final_error');
    });

    it("stream body read failures record safe diagnostics before surfacing a friendly error", () => {
      const s = read("src-tauri/src/ai_runtime/model_gateway/streaming.rs");

      expect(s).toContain("StreamReadFailureDiagnostic");
      expect(s).toContain('"event": "stream_body_read_failed"');
      expect(s).toContain("content_encoding");
      expect(s).toContain("transfer_encoding");
      expect(s).toContain("chunk_count");
      expect(s).toContain("byte_count");
      expect(s).toContain("sse_line_count");
      expect(s).toContain("saw_done");
      expect(s).toContain("visible_partial");
      expect(s).toContain("StreamReadErrorDiagnostic::from_reqwest");
      expect(s).toContain("is_timeout");
      expect(s).toContain("is_body");
      expect(s).toContain("is_connect");
      expect(s).toContain("is_decode");
      expect(s).toContain("stream body read failed");
      expect(s).toContain("模型流式连接中断，请稍后重试或切换模型。");
    });
  });

  describe("Fix 9: web evidence fetch has a turn budget", () => {
    it("web evidence broker stops page extraction when the turn budget is exhausted", () => {
      const s = read("src-tauri/src/ai_runtime/web_evidence_broker.rs");

      expect(s).toContain("WEB_FETCH_TURN_BUDGET");
      expect(s).toContain("fetch_deadline");
      expect(s).toContain("web_page_fetch_budget_exhausted");
    });
  });

  describe("Fix 10: harness phase traces expose timings to the UI", () => {
    it("harness trace events carry duration_ms for timed phases", () => {
      const types = read("src-tauri/src/ai_harness/harness/types.rs");
      const trace = read("src-tauri/src/ai_harness/harness/trace_emit.rs");
      const run = read("src-tauri/src/ai_harness/harness/run.rs");

      expect(types).toContain("duration_ms");
      expect(trace).toContain("duration_ms");
      expect(run).toContain("Some(result.duration_ms)");
    });
  });
});
