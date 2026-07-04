import {
  createEditorIngestFallback,
  ingestMarkdownForEditorSafely,
  type EditorIngestOptions,
  type EditorIngestResult,
} from "@/lib/editor-ingest";
import type { MarkdownSyntaxFragment } from "@/lib/markdown-contract/types";

export const EDITOR_INGEST_WORKER_THRESHOLD_BYTES = 50 * 1024;

interface WorkerIngestResponse {
  tipTapHtml?: string;
  preserveFragments?: MarkdownSyntaxFragment[];
  error?: string;
  requestId: number;
}

interface IngestAsyncOptions {
  createWorker?: () => Worker;
}

let nextRequestId = 0;

function createMarkdownIngestWorker(): Worker {
  return new Worker(
    new URL("../workers/markdown-ingest.worker.ts", import.meta.url),
    {
      type: "module",
    },
  );
}

function shouldUseWorker(bodyMarkdown: string): boolean {
  return (
    bodyMarkdown.length > EDITOR_INGEST_WORKER_THRESHOLD_BYTES &&
    typeof Worker !== "undefined"
  );
}

export function ingestMarkdownForEditorAsync(
  options: EditorIngestOptions,
  asyncOptions: IngestAsyncOptions = {},
): Promise<EditorIngestResult> {
  if (!shouldUseWorker(options.bodyMarkdown) && !asyncOptions.createWorker) {
    return Promise.resolve(ingestMarkdownForEditorSafely(options));
  }

  if (
    options.bodyMarkdown.length <= EDITOR_INGEST_WORKER_THRESHOLD_BYTES &&
    asyncOptions.createWorker
  ) {
    return Promise.resolve(ingestMarkdownForEditorSafely(options));
  }

  const requestId = (nextRequestId += 1);
  let worker: Worker;
  try {
    worker = (asyncOptions.createWorker ?? createMarkdownIngestWorker)();
  } catch (error: unknown) {
    return Promise.resolve(
      createEditorIngestFallback(options.bodyMarkdown, error),
    );
  }

  return new Promise((resolve) => {
    const finish = () => {
      worker.onmessage = null;
      worker.onerror = null;
      worker.terminate();
    };

    const resolveFallback = (reason: unknown) => {
      finish();
      resolve(createEditorIngestFallback(options.bodyMarkdown, reason));
    };

    worker.onmessage = (event: MessageEvent<WorkerIngestResponse>) => {
      const data = event.data;
      if (data.requestId !== requestId) return;
      finish();
      if (data.error) {
        resolve(createEditorIngestFallback(options.bodyMarkdown, data.error));
        return;
      }
      resolve({
        tipTapHtml: data.tipTapHtml ?? "<p></p>",
        preserveFragments: data.preserveFragments ?? [],
        warnings: [],
      });
    };

    worker.onerror = (event) => {
      resolveFallback(
        event.message || "Markdown ingest worker failed before responding",
      );
    };

    try {
      worker.postMessage({
        bodyMarkdown: options.bodyMarkdown,
        requestId,
      });
    } catch (error: unknown) {
      resolveFallback(error);
    }
  });
}
