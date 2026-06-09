// Web Worker for off-main-thread markdown ingestion.
// Offloads ingestMarkdownForEditor for large (>50KB) markdown content
// so the main thread stays responsive during note loading.
import { ingestMarkdownForEditor } from "@/lib/editor-ingest";

export interface IngestRequest {
  bodyMarkdown: string;
  requestId: number;
}

export interface IngestResponse {
  tipTapHtml: string;
  preserveFragments: unknown[];
  requestId: number;
}

self.onmessage = (event: MessageEvent<IngestRequest>) => {
  const { bodyMarkdown, requestId } = event.data;
  try {
    const result = ingestMarkdownForEditor({ bodyMarkdown });
    const response: IngestResponse = {
      tipTapHtml: result.tipTapHtml,
      preserveFragments: result.preserveFragments,
      requestId,
    };
    self.postMessage(response);
  } catch (error: unknown) {
    const msg = error instanceof Error ? error.message : String(error);
    self.postMessage({ error: msg, requestId });
  }
};
