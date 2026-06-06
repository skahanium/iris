import type { Editor } from "@tiptap/core";
import type { NodeViewProps } from "@tiptap/react";
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AiNodeView } from "@/components/editor/AiNodeView";

function streamingNodeViewProps(): NodeViewProps {
  const editor = {
    commands: {
      dismissAiStream: vi.fn(),
      acceptAiStream: vi.fn(),
    },
    extensionManager: {
      extensions: [{ name: "aiStream", options: { onRetry: vi.fn() } }],
    },
  } as unknown as Editor;

  return {
    editor,
    node: {
      attrs: { status: "streaming", action: "translate" },
      textContent: "部分译文",
    },
    decorations: [],
    selected: false,
    extension: {} as NodeViewProps["extension"],
    getPos: () => 0,
    updateAttributes: vi.fn(),
    deleteNode: vi.fn(),
    view: {} as NodeViewProps["view"],
    innerDecorations: {
      type: "decoration",
      decorations: [],
    } as unknown as NodeViewProps["innerDecorations"],
    HTMLAttributes: {},
  } as unknown as NodeViewProps;
}

describe("AiNodeView", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("keeps dismiss enabled while streaming; accept and retry disabled", () => {
    act(() => {
      root.render(createElement(AiNodeView, streamingNodeViewProps()));
    });

    const dismiss = host.querySelector(
      '[data-testid="ai-stream-dismiss"]',
    ) as HTMLButtonElement;
    const accept = host.querySelector(
      '[data-testid="ai-stream-accept"]',
    ) as HTMLButtonElement;
    const retry = host.querySelector(
      '[data-testid="ai-stream-retry"]',
    ) as HTMLButtonElement;

    expect(dismiss.disabled).toBe(false);
    expect(accept.disabled).toBe(true);
    expect(retry.disabled).toBe(true);
  });
});
