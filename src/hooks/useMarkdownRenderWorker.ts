import { useEffect, useRef, useState } from "react";

import { createWorkerRenderableContent } from "@/lib/assistant-render-budget";
import type {
  MarkdownRenderWorkerRequest,
  MarkdownRenderWorkerResponse,
} from "@/lib/markdown-render-worker-core";

interface UseMarkdownRenderWorkerOptions {
  content: string;
  enabled: boolean;
  streaming: boolean;
}

interface MarkdownWorkerState {
  failed: boolean;
  html: string | null;
  pending: boolean;
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

export function useMarkdownRenderWorker({
  content,
  enabled,
  streaming,
}: UseMarkdownRenderWorkerOptions): MarkdownWorkerState {
  const workerRef = useRef<Worker | null>(null);
  const requestIdRef = useRef(0);
  const lastHtmlRef = useRef<string | null>(null);
  const [state, setState] = useState<MarkdownWorkerState>({
    failed: false,
    html: null,
    pending: false,
  });

  useEffect(() => {
    if (!enabled || !streaming) {
      safeTerminate(workerRef.current);
      workerRef.current = null;
      lastHtmlRef.current = null;
      setState({
        failed: false,
        html: null,
        pending: false,
      });
      return;
    }

    let disposed = false;
    const failRender = () => {
      if (disposed) return;
      setState({
        failed: true,
        html: lastHtmlRef.current,
        pending: false,
      });
    };

    if (typeof Worker === "undefined") {
      safeTerminate(workerRef.current);
      workerRef.current = null;
      lastHtmlRef.current = null;
      failRender();
      return () => {
        disposed = true;
      };
    }

    if (!workerRef.current) {
      try {
        workerRef.current = createMarkdownRenderWorker();
      } catch {
        workerRef.current = null;
        failRender();
        return () => {
          disposed = true;
        };
      }
    }

    const worker = workerRef.current;
    const id = requestIdRef.current + 1;
    requestIdRef.current = id;
    setState((prev) => ({
      failed: false,
      html: prev.html ?? lastHtmlRef.current,
      pending: true,
    }));

    worker.onmessage = (event: MessageEvent<MarkdownRenderWorkerResponse>) => {
      if (disposed) return;
      const response = event.data;
      if (response.id !== requestIdRef.current) return;

      if (response.type === "rendered") {
        lastHtmlRef.current = response.html;
        setState({
          failed: false,
          html: response.html,
          pending: false,
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
      }));
    };

    worker.onerror = () => {
      failRender();
      if (workerRef.current === worker) {
        safeTerminate(workerRef.current);
        workerRef.current = null;
      }
    };

    const renderable = createWorkerRenderableContent(content);
    const request: MarkdownRenderWorkerRequest = {
      type: "render",
      id,
      profile: "chat_assistant",
      content: renderable.content,
      streaming,
    };

    try {
      worker.postMessage(request);
    } catch {
      failRender();
      if (workerRef.current === worker) {
        safeTerminate(workerRef.current);
        workerRef.current = null;
      }
    }

    return () => {
      disposed = true;
      try {
        worker.postMessage({ type: "abort", id });
      } catch {
        if (workerRef.current === worker) {
          safeTerminate(workerRef.current);
          workerRef.current = null;
        }
      }
    };
  }, [content, enabled, streaming]);

  useEffect(() => {
    return () => {
      safeTerminate(workerRef.current);
      workerRef.current = null;
      lastHtmlRef.current = null;
    };
  }, []);

  return state;
}
