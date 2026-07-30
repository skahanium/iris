/// <reference lib="webworker" />

import {
  markdownContentHash,
  renderMarkdownForWorker,
  type MarkdownRenderWorkerRequest,
  type MarkdownRenderWorkerResponse,
} from "@/lib/markdown-render-worker-core";

let lastRenderedHash: string | null = null;
let lastRenderedResponse: Extract<
  MarkdownRenderWorkerResponse,
  { type: "rendered" }
> | null = null;
const abortedIds = new Set<number>();

function pruneAbortedIds(): void {
  if (abortedIds.size > 64) abortedIds.clear();
}

function post(response: MarkdownRenderWorkerResponse): void {
  self.postMessage(response);
}

self.onmessage = (event: MessageEvent<MarkdownRenderWorkerRequest>) => {
  pruneAbortedIds();
  const request = event.data;

  if (request.type === "abort") {
    abortedIds.add(request.id);
    post({ type: "skipped", id: request.id, reason: "aborted" });
    return;
  }

  if (abortedIds.has(request.id)) {
    post({ type: "skipped", id: request.id, reason: "aborted" });
    return;
  }

  const contentHash = markdownContentHash(request.content);
  if (contentHash === lastRenderedHash) {
    if (lastRenderedResponse) {
      post({ ...lastRenderedResponse, id: request.id });
      return;
    }
    post({ type: "skipped", id: request.id, reason: "duplicate" });
    return;
  }

  const response = renderMarkdownForWorker(request);
  if (abortedIds.has(request.id)) {
    post({ type: "skipped", id: request.id, reason: "aborted" });
    return;
  }

  if (response.type === "rendered") {
    lastRenderedHash = response.contentHash;
    lastRenderedResponse = response;
  }
  post(response);
};
