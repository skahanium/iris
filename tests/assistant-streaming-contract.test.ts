import { readFileSync, existsSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("AI streaming end-to-end contract (A + B + C)", () => {
  it("exposes an LLM_RESET event constant and typed listener", () => {
    const events = read("src/lib/ipc-events.ts");
    expect(events).toContain('LLM_RESET: "llm:reset"');

    const ipc = read("src/lib/ipc.ts");
    expect(ipc).toContain("listenLlmReset");
  });

  it("useAssistantLlmStream subscribes to llm:reset and clears on reset", () => {
    const hook = read("src/hooks/useAssistantLlmStream.ts");
    expect(hook).toContain("listenLlmReset");
  });

  it("useAssistantLlmStream does not end streaming on llm:done", () => {
    const hook = read("src/hooks/useAssistantLlmStream.ts");
    const doneBlock = hook.split("listenLlmDone")[1] ?? "";
    expect(doneBlock).not.toContain("setStreaming(false)");
  });

  describe("A: harness main loop + reflection stream the terminal answer", () => {
    it("run.rs main loop uses streaming request (not send_request)", () => {
      const run = read("src-tauri/src/ai_harness/harness/run.rs");
      // The agent-round LLM call must go through the streaming path.
      expect(run).toContain("send_streaming_request");
      // A retry wrapper around the streaming call should exist.
      expect(run).toContain("send_llm_streaming_request_with_retry");
    });

    it("run.rs emits llm:reset after non-terminal (tool-call) rounds", () => {
      const run = read("src-tauri/src/ai_harness/harness/run.rs");
      expect(run).toContain("emit_stream_reset");
    });

    it("reflection.rs streams the reflection answer", () => {
      const reflection = read("src-tauri/src/ai_harness/harness/reflection.rs");
      expect(reflection).toContain("send_streaming_request");
      // Non-terminal reflection outcomes must reset leaked tokens.
      expect(reflection).toContain("emit_stream_reset");
    });

    it("streaming.rs exposes an emit_stream_reset helper", () => {
      const streaming = read(
        "src-tauri/src/ai_runtime/model_gateway/streaming.rs",
      );
      expect(streaming).toContain("fn emit_stream_reset");
      expect(streaming).toContain("llm:reset");
    });
  });

  describe("B: research summary streams; internal rounds do not leak tokens", () => {
    it("synthesize_summary uses the streaming path", () => {
      const wf = read("src-tauri/src/ai_workflows/research_workflow.rs");
      const fnBody = wf.split("async fn synthesize_summary")[1] ?? "";
      expect(fnBody).toContain("send_streaming_request");
    });

    it("internal research rounds (decompose/detect/agent) are non-streaming", () => {
      const wf = read("src-tauri/src/ai_workflows/research_workflow.rs");
      const decompose =
        wf.split("async fn decompose_topic")[1]?.split("\n}\n")[0] ?? "";
      const detect =
        wf.split("async fn detect_argument_chains")[1]?.split("\n}\n")[0] ?? "";
      expect(decompose).toContain("send_request");
      expect(decompose).not.toContain("send_streaming_request");
      expect(detect).toContain("send_request");
      expect(detect).not.toContain("send_streaming_request");
    });

    it("runResearch activates the stream slot before awaiting IPC", () => {
      const tasks = read("src/components/ai/hooks/useAssistantTasks.ts");
      const fnBody = tasks.split("const runResearch = useCallback")[1] ?? "";
      expect(fnBody).toContain("ensureAssistantStreamSlot");
      expect(fnBody).toContain("panelSendActiveRef.current = true");
      // request_id must be set before the IPC await so tokens aren't dropped.
      const beforeAwait = fnBody.split("await assistantExecute")[0];
      expect(beforeAwait).toContain("panelSendActiveRef.current = true");
    });
  });

  describe("C: document analysis_summary streams into the doc panel", () => {
    it("enhance_document_check_with_llm streams the summary", () => {
      const wf = read("src-tauri/src/ai_workflows/document_workflow.rs");
      const fnBody =
        wf.split("async fn enhance_document_check_with_llm")[1] ?? "";
      expect(fnBody).toContain("send_streaming_request");
      expect(fnBody).toContain("result.request_id");
    });

    it("document patches stay non-streaming (structured JSON)", () => {
      const wf = read("src-tauri/src/ai_workflows/document_workflow.rs");
      // Isolate generate_llm_document_patches' body (stop at the next fn).
      const after = wf.split("async fn generate_llm_document_patches")[1] ?? "";
      const fnBody = after.split(/^(?:pub )?(?:async )?fn /m)[0] ?? "";
      expect(fnBody).toContain("send_request");
      expect(fnBody).not.toContain("send_streaming_request");
    });

    it("exposes a useDocSummaryStream hook and runDocumentCheck activates it", () => {
      expect(existsSync("src/hooks/useDocSummaryStream.ts")).toBe(true);
      const hook = read("src/hooks/useDocSummaryStream.ts");
      expect(hook).toContain("listenLlmToken");
      expect(hook).toContain("setDocSummary");

      const tasks = read("src/components/ai/hooks/useAssistantTasks.ts");
      const fnBody =
        tasks.split("const runDocumentCheck = useCallback")[1] ?? "";
      // Document streams into the doc panel via a dedicated active ref, not
      // the chat panelSendActiveRef (which would route tokens to the chat list).
      expect(fnBody).toContain("docStreamActiveRef.current = true");
    });
  });
});
