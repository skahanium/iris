import { describe, expect, it, vi } from "vitest";

import {
  EDITOR_INGEST_WORKER_THRESHOLD_BYTES,
  ingestMarkdownForEditorAsync,
} from "@/lib/editor-ingest-async";

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
});
