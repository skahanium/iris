import { describe, expect, it } from "vitest";

import { renderMarkdownWithProfile } from "@/lib/markdown-contract";
import { renderMarkdownForWorker } from "@/lib/markdown-render-worker-core";

const fixtures = [
  "Hello **world**",
  "See [citation:1].",
  "```ts\nconst x = 1;\n```",
  "| A | B |\n| --- | --- |\n| 1 | 2 |",
  "**partial",
];

describe("markdown render worker core", () => {
  it("matches chat_assistant sync output for core fixtures", () => {
    for (const content of fixtures) {
      const streaming = content === "**partial";
      const sync = renderMarkdownWithProfile(content, "chat_assistant", {
        streaming,
      });
      const worker = renderMarkdownForWorker({
        id: 1,
        profile: "chat_assistant",
        content,
        streaming,
        type: "render",
      });

      expect(worker.type).toBe("rendered");
      if (worker.type === "rendered") {
        expect(worker.html).toBe(sync.output);
        expect(worker.renderedLength).toBe(content.length);
      }
    }
  });

  it("returns a stable hash for identical content and different hash for changed content", () => {
    const first = renderMarkdownForWorker({
      id: 1,
      profile: "chat_assistant",
      content: "**same**",
      streaming: true,
      type: "render",
    });
    const second = renderMarkdownForWorker({
      id: 2,
      profile: "chat_assistant",
      content: "**same**",
      streaming: true,
      type: "render",
    });
    const changed = renderMarkdownForWorker({
      id: 3,
      profile: "chat_assistant",
      content: "**changed**",
      streaming: true,
      type: "render",
    });

    expect(first.type).toBe("rendered");
    expect(second.type).toBe("rendered");
    expect(changed.type).toBe("rendered");
    if (
      first.type === "rendered" &&
      second.type === "rendered" &&
      changed.type === "rendered"
    ) {
      expect(second.contentHash).toBe(first.contentHash);
      expect(changed.contentHash).not.toBe(first.contentHash);
    }
  });
});
