import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { IrisContextMenu } from "@/components/ui/iris-context-menu";

let root: Root | null = null;
let host: HTMLDivElement | null = null;

function renderMenu(onClose = vi.fn()) {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  const groups = [
    {
      group: "AI",
      items: Array.from({ length: 18 }, (_, index) => ({
        id: `item-${index}`,
        label: `Item ${index}`,
      })),
    },
  ];

  act(() => {
    root?.render(
      <IrisContextMenu
        open
        x={24}
        y={24}
        groups={groups}
        onSelect={vi.fn()}
        onClose={onClose}
      />,
    );
  });

  return onClose;
}

afterEach(() => {
  act(() => root?.unmount());
  root = null;
  host?.remove();
  host = null;
  document.body.innerHTML = "";
});

describe("IrisContextMenu", () => {
  it("keeps the menu open when scrolling its own overflow panel", () => {
    const onClose = renderMenu();
    const menu = document.querySelector<HTMLElement>('[role="menu"]');

    expect(menu).not.toBeNull();
    menu?.dispatchEvent(new Event("scroll", { bubbles: true }));

    expect(onClose).not.toHaveBeenCalled();
  });

  it("closes the menu when the page behind it scrolls", () => {
    const onClose = renderMenu();

    window.dispatchEvent(new Event("scroll"));

    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
