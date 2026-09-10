import { existsSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("assistant stream rendering performance contract", () => {
  it("sends each committed streaming delta directly to the isolated tail", () => {
    const src = read("src/components/ai/AiMessageBubble.tsx");

    expect(src).toContain(
      "content={streaming ? streamingContent : finalizedContent}",
    );
    expect(src).toContain("createStreamingRenderableContent(content)");
    expect(src).not.toContain("useStreamingContent");
    expect(src).not.toContain("useMarkdownRenderWorker");
  });

  it("removes the obsolete streaming content throttle module", () => {
    expect(existsSync("src/hooks/useStreamingContent.ts")).toBe(false);
  });

  it("uses the reveal hook for large backlogs instead of a whole-delta throttle", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");
    const reveal = read("src/components/ai/hooks/useAssistantAnswerReveal.ts");

    const bubble = read("src/components/ai/AiMessageBubble.tsx");
    expect(bubble).toContain("useAssistantAnswerReveal");
    expect(bubble).toContain("StreamingLineBudgetRefContext.Provider");
    expect(panel).not.toContain("presentationReveal");
    expect(panel).not.toContain("useAssistantAnswerReveal");
    expect(panel).toContain("scheduleComposerClear");
    expect(panel).toContain("composerClearEpoch");
    expect(panel).not.toContain("clearComposer:");
    expect(reveal).toContain("nextRevealLength");
    expect(reveal).not.toContain("ASSISTANT_ANSWER_REVEAL_MIN_STEP = 2");
    expect(reveal).not.toContain("ASSISTANT_ANSWER_REVEAL_IMMEDIATE_CHARS");
    const body = read("src/components/ai/WindowedMarkdownBlock.tsx");
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
    expect(tailRule).not.toContain("white-space: pre-wrap");
    expect(tailRule).not.toContain("min-height");
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

  it("gives measured block placeholders sole ownership of offscreen height", () => {
    const css = read("src/styles/globals.css");
    expect(css).not.toContain("contain-intrinsic-size: auto 320px");
    expect(css).toContain("overflow-anchor: none");
    expect(read("src/components/ai/AiMessageBubble.tsx")).not.toContain(
      "<FinalizedMessageBody",
    );
  });

  it("assistant bubbles expose stable data attributes for finalized and streaming states", () => {
    const src = read("src/components/ai/AiMessageBubble.tsx");

    expect(src).toContain("data-role={role}");
    expect(src).toContain('data-streaming={streaming ? "" : undefined}');
  });
});
