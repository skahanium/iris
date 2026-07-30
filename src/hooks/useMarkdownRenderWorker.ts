import { useEffect, useRef, useState } from "react";

import { createWorkerRenderableContent } from "@/lib/assistant-render-budget";
import type {
  MarkdownRenderWorkerRequest,
  MarkdownRenderWorkerResponse,
} from "@/lib/markdown-render-worker-core";
import { markdownContentHash } from "@/lib/markdown-render-worker-core";

interface UseMarkdownRenderWorkerOptions {
  content: string;
  enabled: boolean;
  streaming: boolean;
}

interface MarkdownWorkerState {
  failed: boolean;
  html: string | null;
  pending: boolean;
  contentHash: string;
}

function createMarkdownRenderWorker(): Worker {
  return new Worker(
    new URL("../workers/markdown-render.worker.ts", import.meta.url),
    { type: "module" },
  );
}

function safeTerminate(worker: Worker | null): void {
  if (!worker) return;
  try {
    worker.terminate();
  } catch {
    // Ignore worker shutdown errors from a half-closed WebView worker port.
  }
}

type WorkerListener = (response: MarkdownRenderWorkerResponse) => void;

let sharedWorker: Worker | null = null;
let sharedRequestId = 0;
let sharedSubscriptions = 0;
const sharedListeners = new Map<number, WorkerListener>();

function failSharedWorker(worker: Worker | null): void {
  if (!worker || sharedWorker !== worker) return;
  safeTerminate(worker);
  sharedWorker = null;
  const listeners = Array.from(sharedListeners.values());
  sharedListeners.clear();
  for (const listener of listeners) {
    listener({ type: "error", id: -1, message: "Markdown Worker 不可用" });
  }
}

function acquireSharedWorker(): Worker {
  if (sharedWorker) {
    sharedSubscriptions += 1;
    return sharedWorker;
  }
  const worker = createMarkdownRenderWorker();
  worker.onmessage = (event: MessageEvent<MarkdownRenderWorkerResponse>) => {
    const response = event.data;
    const listener = sharedListeners.get(response.id);
    if (!listener) return;
    sharedListeners.delete(response.id);
    listener(response);
  };
  worker.onerror = () => failSharedWorker(worker);
  sharedWorker = worker;
  sharedSubscriptions += 1;
  return worker;
}

function releaseSharedWorker(worker: Worker): void {
  sharedSubscriptions = Math.max(0, sharedSubscriptions - 1);
  queueMicrotask(() => {
    if (sharedSubscriptions === 0 && sharedWorker === worker) {
      safeTerminate(worker);
      sharedWorker = null;
    }
  });
}

export function useMarkdownRenderWorker({
  content,
  enabled,
  streaming,
}: UseMarkdownRenderWorkerOptions): MarkdownWorkerState {
  const lastHtmlRef = useRef<string | null>(null);
  const contentHash = markdownContentHash(content);
  const [state, setState] = useState<MarkdownWorkerState>({
    failed: false,
    html: null,
    pending: enabled,
    contentHash,
  });

  useEffect(() => {
    if (!enabled) {
      lastHtmlRef.current = null;
      setState({
        failed: false,
        html: null,
        pending: false,
        contentHash,
      });
      return;
    }

    let disposed = false;
    const failRender = () => {
      if (disposed) return;
      setState({
        failed: true,
        html: null,
        pending: false,
        contentHash,
      });
    };

    if (typeof Worker === "undefined") {
      failRender();
      return () => {
        disposed = true;
      };
    }

    let worker: Worker;
    try {
      worker = acquireSharedWorker();
    } catch {
      failRender();
      return () => {
        disposed = true;
      };
    }
    const id = sharedRequestId + 1;
    sharedRequestId = id;
    setState({
      failed: false,
      html: null,
      pending: true,
      contentHash,
    });

    const receive = (response: MarkdownRenderWorkerResponse) => {
      if (disposed) return;

      if (response.type === "rendered") {
        lastHtmlRef.current = response.html;
        setState({
          failed: false,
          html: response.html,
          pending: false,
          contentHash,
        });
        return;
      }

      if (response.type === "error") {
        failRender();
        return;
      }

      setState((prev) => ({
        failed: false,
        html: prev.html ?? lastHtmlRef.current,
        pending: false,
        contentHash,
      }));
    };

    const renderable = streaming
      ? createWorkerRenderableContent(content)
      : { content };
    const request: MarkdownRenderWorkerRequest = {
      type: "render",
      id,
      profile: "chat_assistant",
      content: renderable.content,
      streaming,
    };

    try {
      sharedListeners.set(id, receive);
      worker.postMessage(request);
    } catch {
      sharedListeners.delete(id);
      failRender();
      failSharedWorker(worker);
    }

    return () => {
      disposed = true;
      sharedListeners.delete(id);
      try {
        worker.postMessage({ type: "abort", id });
      } catch {
        failSharedWorker(worker);
      }
      releaseSharedWorker(worker);
    };
  }, [content, contentHash, enabled, streaming]);

  if (state.contentHash === contentHash) return state;
  return {
    failed: false,
    html: null,
    pending: enabled,
    contentHash,
  };
}
