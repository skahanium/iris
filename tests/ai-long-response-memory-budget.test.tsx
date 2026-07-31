import { readFileSync } from "node:fs";

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { AiMessageBubble } from "@/components/ai/AiMessageBubble";

const LONG_TEXT_LENGTH = 220_000;
const RENDERED_TEXT_BUDGET = 60_000;

function longMarkdown(): string {
  return [
    "# Long answer",
    "",
    "A".repeat(LONG_TEXT_LENGTH),
    "",
    "```txt",
    "B".repeat(80_000),
    "```",
  ].join("\n");
}

describe("AI long response memory budget", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("renders only a bounded window for very long assistant messages", async () => {
    const content = longMarkdown();

    await act(async () => {
      root.render(<AiMessageBubble role="assistant" content={content} />);
    });

    expect(content.length).toBeGreaterThan(250_000);
    expect(host.textContent?.length ?? 0).toBeLessThan(RENDERED_TEXT_BUDGET);
    expect(host.textContent).toContain("Long answer");
    expect(host.textContent).toContain("truncated");
  });

  it("streams only a bounded tail window for very long assistant messages", async () => {
    const content = longMarkdown();

    await act(async () => {
      root.render(
        <AiMessageBubble role="assistant" content={content} streaming />,
      );
    });

    expect(host.textContent?.length ?? 0).toBeLessThan(40_000);
    expect(host.textContent).toContain("truncated");
  });

  it("keeps the bounded render budget under the focus content column", () => {
    const css = readFileSync("src/styles/globals.css", "utf8");
    const bubble = readFileSync(
      "src/components/ai/AiMessageBubble.tsx",
      "utf8",
    );

    // 主区阅读内容列与文档保护宽度同源（--prose-measure），不硬编码 832px。
    expect(css).toContain(".ai-focus-column");
    expect(css).toContain("max-width: var(--prose-measure)");
    // 长回答预算仍作用于 focus 列内的气泡，不因加宽被绕过。
    expect(bubble).toContain("createRenderableAssistantContent");
    expect(bubble).not.toContain("max-width: none");
  });
});
