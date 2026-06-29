import { useEffect, useRef, useState } from "react";

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
      workerRef.current?.terminate();
      workerRef.current = null;
      setState({
        failed: false,
        html: null,
        pending: false,
      });
      return;
    }

    if (typeof Worker === "undefined") {
      workerRef.current?.terminate();
      workerRef.current = null;
      setState({
        failed: true,
        html: lastHtmlRef.current,
        pending: false,
      });
      return;
    }

    if (!workerRef.current) {
      workerRef.current = createMarkdownRenderWorker();
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
        setState({
          failed: true,
          html: lastHtmlRef.current,
          pending: false,
        });
        return;
      }

      setState((prev) => ({
        failed: false,
        html: prev.html ?? lastHtmlRef.current,
        pending: false,
      }));
    };

    worker.onerror = () => {
      setState({
        failed: true,
        html: lastHtmlRef.current,
        pending: false,
      });
    };

    const request: MarkdownRenderWorkerRequest = {
      type: "render",
      id,
      profile: "chat_assistant",
      content,
      streaming,
    };
    worker.postMessage(request);

    return () => {
      worker.postMessage({ type: "abort", id });
    };
  }, [content, enabled, streaming]);

  useEffect(() => {
    return () => {
      workerRef.current?.terminate();
      workerRef.current = null;
    };
  }, []);

  return state;
}
