import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useMarkdownRenderWorker } from "@/hooks/useMarkdownRenderWorker";

function WorkerProbe({ content = "# hi" }: { content?: string }) {
  const state = useMarkdownRenderWorker({
    content,
    enabled: true,
    streaming: true,
  });

  return (
    <output
      data-failed={String(state.failed)}
      data-pending={String(state.pending)}
    >
      {state.html ?? ""}
    </output>
  );
}

describe("useMarkdownRenderWorker", () => {
  let host: HTMLDivElement;
  let root: Root;
  let originalWorker: typeof Worker | undefined;

  beforeEach(() => {
    originalWorker = globalThis.Worker;
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    if (originalWorker === undefined) {
      Reflect.deleteProperty(globalThis, "Worker");
    } else {
      Object.defineProperty(globalThis, "Worker", {
        configurable: true,
        writable: true,
        value: originalWorker,
      });
    }
    vi.restoreAllMocks();
  });

  it("reports failure instead of throwing when worker construction fails", async () => {
    class ThrowingWorker {
      constructor() {
        throw new Error("worker unavailable");
      }
    }
    Object.defineProperty(globalThis, "Worker", {
      configurable: true,
      writable: true,
      value: ThrowingWorker,
    });

    await act(async () => {
      root.render(<WorkerProbe />);
    });

    const output = host.querySelector("output");
    expect(output?.dataset.failed).toBe("true");
    expect(output?.dataset.pending).toBe("false");
  });

  it("reports failure instead of throwing when worker postMessage fails", async () => {
    class FailingPostWorker {
      onerror: ((event: Event) => void) | null = null;
      onmessage: ((event: MessageEvent) => void) | null = null;

      postMessage() {
        throw new Error("port closed");
      }

      terminate() {}
    }
    Object.defineProperty(globalThis, "Worker", {
      configurable: true,
      writable: true,
      value: FailingPostWorker,
    });

    await act(async () => {
      root.render(<WorkerProbe />);
    });

    const output = host.querySelector("output");
    expect(output?.dataset.failed).toBe("true");
    expect(output?.dataset.pending).toBe("false");
  });
  it("sends only a bounded render window to the worker for very long streaming content", async () => {
    const posted: unknown[] = [];
    class CapturingWorker {
      onerror: ((event: Event) => void) | null = null;
      onmessage: ((event: MessageEvent) => void) | null = null;

      postMessage(message: unknown) {
        posted.push(message);
      }

      terminate() {}
    }
    Object.defineProperty(globalThis, "Worker", {
      configurable: true,
      writable: true,
      value: CapturingWorker,
    });

    await act(async () => {
      root.render(<WorkerProbe content={"A".repeat(240_000)} />);
    });

    const renderRequest = posted.find(
      (message): message is { content: string; type: string } =>
        Boolean(
          message &&
          typeof message === "object" &&
          "type" in message &&
          (message as { type?: unknown }).type === "render",
        ),
    );
    expect(renderRequest?.content.length).toBeLessThanOrEqual(40_000);
  });
});
