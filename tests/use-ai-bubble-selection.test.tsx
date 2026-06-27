import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { useAiBubbleSelection } from "@/hooks/useAiBubbleSelection";
import type { ContextReference } from "@/types/ai";

type HookApi = ReturnType<typeof useAiBubbleSelection>;

const reference: ContextReference = {
  id: "selection:notes/current.md:hash:full:full",
  kind: "selection",
  filePath: "notes/current.md",
  contentHash: "hash",
  utf8Range: null,
  editorRange: { from: 1, to: 8 },
  excerpt: "selected text",
  headingPath: null,
  anchor: null,
  stale: false,
};

function Harness({
  onReady,
  onRender,
}: {
  onReady: (api: HookApi) => void;
  onRender: () => void;
}) {
  const api = useAiBubbleSelection();
  onRender();
  onReady(api);
  return null;
}

describe("useAiBubbleSelection", () => {
  let container: HTMLDivElement;
  let root: Root;
  let api!: HookApi;
  let renderCount = 0;

  beforeEach(async () => {
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    renderCount = 0;
    await act(async () => {
      root.render(
        createElement(Harness, {
          onReady: (value) => {
            api = value;
          },
          onRender: () => {
            renderCount += 1;
          },
        }),
      );
    });
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("keeps the reference list idempotent when quoting the same context twice", async () => {
    expect(renderCount).toBe(1);

    await act(async () => {
      api.quoteSelectionAsReference(reference);
    });

    expect(api.contextReferences).toEqual([reference]);
    const renderCountAfterFirstQuote = renderCount;

    await act(async () => {
      api.quoteSelectionAsReference({ ...reference });
    });

    expect(api.contextReferences).toEqual([reference]);
    expect(renderCount).toBe(renderCountAfterFirstQuote);
  });

  it("toggles individual message selections and supports shift ranges", async () => {
    await act(async () => {
      api.handleClick(1, { shiftKey: false, metaKey: false, ctrlKey: false });
    });

    expect([...api.selected]).toEqual([1]);

    await act(async () => {
      api.handleClick(1, { shiftKey: false, metaKey: false, ctrlKey: false });
    });

    expect([...api.selected]).toEqual([]);

    await act(async () => {
      api.handleClick(1, { shiftKey: false, metaKey: false, ctrlKey: false });
    });
    await act(async () => {
      api.handleClick(3, { shiftKey: true, metaKey: false, ctrlKey: false });
    });

    expect([...api.selected]).toEqual([1, 2, 3]);
  });

  it("prunes selected indices that no longer have messages", async () => {
    await act(async () => {
      api.handleClick(1, { shiftKey: false, metaKey: false, ctrlKey: false });
    });
    await act(async () => {
      api.handleClick(3, { shiftKey: true, metaKey: false, ctrlKey: false });
    });

    expect([...api.selected]).toEqual([1, 2, 3]);

    await act(async () => {
      api.pruneSelected(2);
    });

    expect([...api.selected]).toEqual([1]);
  });
});
