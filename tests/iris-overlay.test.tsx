import { act } from "react";
import type { ReactElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { IrisOverlay } from "@/components/ui/iris-overlay";

let root: Root | null = null;
let host: HTMLDivElement | null = null;

function renderOverlay(element: ReactElement) {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  act(() => {
    root?.render(element);
  });
}

afterEach(() => {
  if (root) {
    act(() => {
      root?.unmount();
    });
  }
  host?.remove();
  root = null;
  host = null;
});

describe("IrisOverlay", () => {
  it("renders a centered dialog with scrim and size classes", () => {
    renderOverlay(
      <IrisOverlay open title="全文搜索" size="command" onClose={() => {}}>
        内容
      </IrisOverlay>,
    );

    const dialog = document.querySelector('[role="dialog"]');
    const scrim = document.querySelector('[data-slot="iris-overlay-scrim"]');

    expect(dialog?.getAttribute("aria-label")).toBe("全文搜索");
    expect(dialog?.className).toContain("z-overlay");
    expect(dialog?.className).toContain("w-[80vw]");
    expect(dialog?.className).toContain("h-[78vh]");
    expect(scrim?.className).toContain("z-overlay-scrim");
    expect(scrim?.className).toContain("bg-overlay-scrim");
  });

  it("closes when the scrim or Escape key is pressed", () => {
    const onClose = vi.fn();
    renderOverlay(
      <IrisOverlay open title="知识图谱" size="graph" onClose={onClose}>
        内容
      </IrisOverlay>,
    );

    const scrim = document.querySelector('[data-slot="iris-overlay-scrim"]');

    act(() => {
      scrim?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(onClose).toHaveBeenCalledTimes(1);

    act(() => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    });
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
