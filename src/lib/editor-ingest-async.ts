import {
  ingestMarkdownForEditor,
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
    return Promise.resolve(ingestMarkdownForEditor(options));
  }

  if (
    options.bodyMarkdown.length <= EDITOR_INGEST_WORKER_THRESHOLD_BYTES &&
    asyncOptions.createWorker
  ) {
    return Promise.resolve(ingestMarkdownForEditor(options));
  }

  const requestId = (nextRequestId += 1);
  const worker = (asyncOptions.createWorker ?? createMarkdownIngestWorker)();

  return new Promise((resolve, reject) => {
    const finish = () => {
      worker.onmessage = null;
      worker.onerror = null;
      worker.terminate();
    };

    worker.onmessage = (event: MessageEvent<WorkerIngestResponse>) => {
      const data = event.data;
      if (data.requestId !== requestId) return;
      finish();
      if (data.error) {
        reject(new Error(data.error));
        return;
      }
      resolve({
        tipTapHtml: data.tipTapHtml ?? "<p></p>",
        preserveFragments: data.preserveFragments ?? [],
        warnings: [],
      });
    };

    worker.onerror = (event) => {
      finish();
      reject(
        new Error(
          event.message || "Markdown ingest worker failed before responding",
        ),
      );
    };

    worker.postMessage({
      bodyMarkdown: options.bodyMarkdown,
      requestId,
    });
  });
}
