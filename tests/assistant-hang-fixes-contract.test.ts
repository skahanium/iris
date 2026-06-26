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

  describe("Fix 4: abort button visible while streaming", () => {
    it("AssistantProcessStatusBar treats streaming as active", () => {
      const s = read("src/components/ai/AssistantProcessStatusBar.tsx");
      // `active` must include streaming so the bar renders at all.
      expect(s).toMatch(
        /isActiveStatus\(agentTask\)\s*\|\|\s*researchRunning\s*\|\|\s*hasError\s*\|\|\s*streaming/,
      );
    });

    it("AssistantProcessStatusBar allows abort while streaming", () => {
      const s = read("src/components/ai/AssistantProcessStatusBar.tsx");
      // canAbort must include streaming so the 中止 button renders.
      const canAbortBlock = s.split("const canAbort")[1] ?? "";
      expect(canAbortBlock).toContain("streaming");
    });
  });

  describe("Fix 6: run_harness has a global deadline", () => {
    it("wraps the harness body in tokio::time::timeout", () => {
      const s = read("src-tauri/src/ai_harness/harness/run.rs");
      expect(s).toContain("tokio::time::timeout");
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
});
