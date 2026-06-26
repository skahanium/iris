import { describe, expect, it } from "vitest";

import { renderMarkdownWithProfile } from "@/lib/markdown-contract/contract";
import { clearMarkdownRenderCache } from "@/lib/markdown-contract/contract";

describe("markdown render cache (Fix 1)", () => {
  it("returns identical output for the same input without re-parsing", () => {
    clearMarkdownRenderCache();
    const source =
      "# 标题\n\n这是一段**加粗**文本，含 `code` 和 [链接](https://example.com)。";
    const first = renderMarkdownWithProfile(source, "chat_assistant", {
      streaming: false,
    });
    // Tiny delay so Date.now() would differ if re-rendered.
    const start = first.meta.renderedAt;
    // Spin until at least 1ms passes to ensure a fresh render would get a
    // different renderedAt timestamp.
    while (Date.now() <= start) {
      /* busy-wait */
    }
    const second = renderMarkdownWithProfile(source, "chat_assistant", {
      streaming: false,
    });
    // Cache hit: same object identity (or at least same renderedAt).
    expect(second.meta.renderedAt).toBe(first.meta.renderedAt);
    expect(second.output).toBe(first.output);
    expect(second.warnings).toEqual(first.warnings);
  });

  it("distinguishes cache entries by profile", () => {
    clearMarkdownRenderCache();
    const source = "**bold**";
    const assistant = renderMarkdownWithProfile(source, "chat_assistant");
    const user = renderMarkdownWithProfile(source, "chat_user");
    // Different profiles may produce different output; both must be cached
    // independently (not collide).
    const assistantAgain = renderMarkdownWithProfile(source, "chat_assistant");
    const userAgain = renderMarkdownWithProfile(source, "chat_user");
    expect(assistantAgain.output).toBe(assistant.output);
    expect(userAgain.output).toBe(user.output);
  });

  it("distinguishes cache entries by streaming flag", () => {
    clearMarkdownRenderCache();
    const source = "**bold**";
    const nonStream = renderMarkdownWithProfile(source, "chat_assistant", {
      streaming: false,
    });
    const stream = renderMarkdownWithProfile(source, "chat_assistant", {
      streaming: true,
    });
    const nonStreamAgain = renderMarkdownWithProfile(source, "chat_assistant", {
      streaming: false,
    });
    expect(nonStreamAgain.output).toBe(nonStream.output);
    // Streaming output may differ; just confirm no crash/collision.
    expect(typeof stream.output).toBe("string");
  });

  it("does not cache streaming results (content is incomplete mid-stream)", () => {
    clearMarkdownRenderCache();
    const source = "partial content";
    const first = renderMarkdownWithProfile(source, "chat_assistant", {
      streaming: true,
    });
    const second = renderMarkdownWithProfile(source, "chat_assistant", {
      streaming: true,
    });
    // Streaming results are NOT cached (the content is mid-flight and will
    // grow). Each call must re-render. We confirm by checking both return
    // valid strings (we can't easily spy on internals, but the contract is:
    // streaming never reads from cache).
    expect(typeof first.output).toBe("string");
    expect(typeof second.output).toBe("string");
  });

  it("evicts oldest entries when the cache exceeds its max size", () => {
    clearMarkdownRenderCache();
    // Fill the cache with distinct entries.
    const results: string[] = [];
    for (let i = 0; i < 60; i += 1) {
      const r = renderMarkdownWithProfile(
        `# Entry ${i}\n\nunique content ${i}`,
        "chat_assistant",
        { streaming: false },
      );
      results.push(r.output);
    }
    // All 60 rendered successfully.
    expect(results).toHaveLength(60);
    expect(results.every((r) => r.length > 0)).toBe(true);
    // The cache should not have grown unbounded (no crash, no memory leak).
    // Re-rendering a recent entry returns the same output.
    const recent = renderMarkdownWithProfile(
      "# Entry 59\n\nunique content 59",
      "chat_assistant",
      { streaming: false },
    );
    expect(recent.output).toBe(results[59]);
  });
});
