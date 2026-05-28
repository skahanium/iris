import { act } from "react";
import { createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useListboxKeyboard } from "@/hooks/useListboxKeyboard";

function Harness({
  length,
  wrap,
  resetKey,
  onReady,
}: {
  length: number;
  wrap?: boolean;
  resetKey?: string;
  onReady: (api: ReturnType<typeof useListboxKeyboard>) => void;
}) {
  const api = useListboxKeyboard({
    length,
    wrap,
    resetKey,
    onActivate: vi.fn(),
  });
  onReady(api);
  return null;
}

describe("useListboxKeyboard", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("clamps highlight at list ends when wrap is false", async () => {
    let api!: ReturnType<typeof useListboxKeyboard>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          length: 3,
          onReady: (value) => {
            api = value;
          },
        }),
      );
    });

    act(() => {
      api.handleKeyDown({ key: "ArrowUp", preventDefault: () => {} });
    });
    expect(api.highlight).toBe(0);

    act(() => {
      api.handleKeyDown({ key: "ArrowDown", preventDefault: () => {} });
      api.handleKeyDown({ key: "ArrowDown", preventDefault: () => {} });
      api.handleKeyDown({ key: "ArrowDown", preventDefault: () => {} });
    });
    expect(api.highlight).toBe(2);
  });

  it("wraps highlight when wrap is true", async () => {
    let api!: ReturnType<typeof useListboxKeyboard>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          length: 3,
          wrap: true,
          onReady: (value) => {
            api = value;
          },
        }),
      );
    });

    act(() => {
      api.handleKeyDown({ key: "ArrowUp", preventDefault: () => {} });
    });
    expect(api.highlight).toBe(2);
  });

  it("resets highlight when resetKey changes", async () => {
    let api!: ReturnType<typeof useListboxKeyboard>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          length: 3,
          resetKey: "a",
          onReady: (value) => {
            api = value;
          },
        }),
      );
    });

    act(() => {
      api.handleKeyDown({ key: "ArrowDown", preventDefault: () => {} });
    });
    expect(api.highlight).toBe(1);

    await act(async () => {
      root.render(
        createElement(Harness, {
          length: 3,
          resetKey: "b",
          onReady: (value) => {
            api = value;
          },
        }),
      );
    });
    expect(api.highlight).toBe(0);
  });
});
