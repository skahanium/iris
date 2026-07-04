import { describe, expect, it, vi } from "vitest";

import {
  EDITOR_INGEST_WORKER_THRESHOLD_BYTES,
  ingestMarkdownForEditorAsync,
} from "@/lib/editor-ingest-async";
import { createEditorIngestFallback } from "@/lib/editor-ingest";

describe("editor ingest async worker routing", () => {
  it("keeps small documents on the synchronous ingest path", async () => {
    const createWorker = vi.fn();

    const result = await ingestMarkdownForEditorAsync(
      { bodyMarkdown: "small note" },
      { createWorker },
    );

    expect(createWorker).not.toHaveBeenCalled();
    expect(result.tipTapHtml).toContain("small note");
  });

  it("uses the worker for large documents and ignores mismatched request ids", async () => {
    let onmessage: ((event: MessageEvent) => void) | null = null;
    const terminate = vi.fn();
    const postMessage = vi.fn((payload: { requestId: number }) => {
      onmessage?.({
        data: {
          requestId: payload.requestId + 1,
          tipTapHtml: "<p>stale</p>",
          preserveFragments: [],
        },
      } as MessageEvent);
      onmessage?.({
        data: {
          requestId: payload.requestId,
          tipTapHtml: "<p>fresh</p>",
          preserveFragments: [],
        },
      } as MessageEvent);
    });
    const worker = {
      postMessage,
      terminate,
      set onmessage(next: ((event: MessageEvent) => void) | null) {
        onmessage = next;
      },
      get onmessage() {
        return onmessage;
      },
      onerror: null,
    } as unknown as Worker;

    const result = await ingestMarkdownForEditorAsync(
      { bodyMarkdown: "x".repeat(EDITOR_INGEST_WORKER_THRESHOLD_BYTES + 1) },
      { createWorker: () => worker },
    );

    expect(postMessage).toHaveBeenCalledOnce();
    expect(terminate).toHaveBeenCalledOnce();
    expect(result.tipTapHtml).toBe("<p>fresh</p>");
  });

  it("returns a raw-preserving fallback when the worker fails", async () => {
    let onerror: ((event: ErrorEvent) => void) | null = null;
    const terminate = vi.fn();
    const postMessage = vi.fn(() => {
      onerror?.({ message: "worker exploded" } as ErrorEvent);
    });
    const worker = {
      postMessage,
      terminate,
      onmessage: null,
      set onerror(next: ((event: ErrorEvent) => void) | null) {
        onerror = next;
      },
      get onerror() {
        return onerror;
      },
    } as unknown as Worker;
    const bodyMarkdown =
      "x".repeat(EDITOR_INGEST_WORKER_THRESHOLD_BYTES + 1) +
      '\n\n<script>alert("keep raw")</script>';

    const result = await ingestMarkdownForEditorAsync(
      { bodyMarkdown },
      { createWorker: () => worker },
    );

    expect(postMessage).toHaveBeenCalledOnce();
    expect(terminate).toHaveBeenCalledOnce();
    expect(result.tipTapHtml).toContain('data-type="preserve-block"');
    expect(result.tipTapHtml).toContain(
      "&lt;script&gt;alert(&quot;keep raw&quot;)&lt;/script&gt;",
    );
    expect(result.preserveFragments).toEqual([
      expect.objectContaining({
        capability: "unsupported",
        raw: bodyMarkdown,
        syntaxKind: "unknown",
      }),
    ]);
    expect(result.warnings[0]?.message).toContain("worker exploded");
  });

  it("builds a fallback that serializes the original markdown body", () => {
    const bodyMarkdown = [
      "# 中华人民共和国监察法实施条例",
      "",
      "正文必须保留。",
      "",
      '<script>alert("preserve")</script>',
    ].join("\n");

    const fallback = createEditorIngestFallback(bodyMarkdown, "parser failed");

    expect(fallback.tipTapHtml).toContain('data-type="preserve-block"');
    expect(fallback.tipTapHtml).toContain("正文必须保留");
    expect(fallback.tipTapHtml).not.toContain("<script>");
    expect(fallback.preserveFragments).toEqual([
      expect.objectContaining({ raw: bodyMarkdown, offset: 0 }),
    ]);
    expect(fallback.warnings[0]?.message).toContain("parser failed");
  });
});
