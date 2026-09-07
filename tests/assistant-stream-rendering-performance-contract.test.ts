import { existsSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("assistant stream rendering performance contract", () => {
  it("sends each committed streaming delta directly to the isolated tail", () => {
    const src = read("src/components/ai/AiMessageBubble.tsx");

    expect(src).toContain("content={streamingContent}");
    expect(src).toContain("createStreamingRenderableContent(content)");
    expect(src).not.toContain("useStreamingContent");
  });

  it("removes the obsolete streaming content throttle module", () => {
    expect(existsSync("src/hooks/useStreamingContent.ts")).toBe(false);
  });

  it("uses the reveal hook for large backlogs instead of a whole-delta throttle", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");
    const reveal = read("src/components/ai/hooks/useAssistantAnswerReveal.ts");

    expect(panel).toContain("useAssistantAnswerReveal");
    expect(panel).toContain("presentationReveal");
    expect(panel).toContain("StreamingLineBudgetRefContext.Provider");
    expect(panel).toContain("scheduleComposerClear");
    expect(panel).toContain("composerClearEpoch");
    expect(panel).not.toContain("clearComposer:");
    expect(reveal).toContain("nextRevealLength");
    expect(reveal).not.toContain("ASSISTANT_ANSWER_REVEAL_MIN_STEP = 2");
    expect(reveal).not.toContain("ASSISTANT_ANSWER_REVEAL_IMMEDIATE_CHARS");
    const body = read("src/components/ai/StreamingMessageBody.tsx");
    expect(body).toContain("readTailLineBudget");
  });

  it("does not clip streaming bubbles or wrap the tail at arbitrary graphemes", () => {
    const bubble = read("src/components/ai/AiMessageBubble.tsx");
    const css = read("src/styles/globals.css");
    const tailAfter = css.split(".ai-streaming-tail")[1] ?? "";
    const tailRule = tailAfter.split("}")[0] ?? "";

    expect(bubble).toContain(
      'streaming ? "overflow-visible" : "overflow-hidden"',
    );
    expect(tailRule).toContain("white-space: pre-wrap");
    expect(tailRule).toContain("overflow-wrap: break-word");
    expect(tailRule).not.toContain("anywhere");
    expect(tailRule).not.toContain("word-break");
  });

  it("contains streaming assistant bubble style work without layout clipping", () => {
    const css = read("src/styles/globals.css");
    const after =
      css.split(".ai-message-bubble-streaming[data-streaming]")[1] ?? "";
    const streamingRule = after.split("}")[0] ?? "";

    expect(streamingRule).toContain("contain: style");
    expect(streamingRule).not.toContain("layout");
    expect(streamingRule).not.toContain("paint");
    expect(streamingRule).not.toContain("content-visibility");
  });

  it("allows content-visibility only for finalized assistant bubbles", () => {
    const css = read("src/styles/globals.css");
    const afterFinalized =
      css.split(".ai-message-bubble-assistant:not([data-streaming])")[1] ?? "";
    const finalizedRule = afterFinalized.split("}")[0] ?? "";
    const afterStreaming =
      css.split(".ai-message-bubble-streaming[data-streaming]")[1] ?? "";
    const streamingRule = afterStreaming.split("}")[0] ?? "";

    expect(finalizedRule).toContain("content-visibility: auto");
    expect(finalizedRule).toContain("contain-intrinsic-size");
    expect(streamingRule).not.toContain("content-visibility: auto");
  });

  it("assistant bubbles expose stable data attributes for finalized and streaming states", () => {
    const src = read("src/components/ai/AiMessageBubble.tsx");

    expect(src).toContain("data-role={role}");
    expect(src).toContain('data-streaming={streaming ? "" : undefined}');
  });
});
