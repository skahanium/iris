import { act, createElement, useRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useAppKeyboard } from "@/hooks/useAppKeyboard";
import type { AppShortcutItem } from "@/lib/app-shortcuts";

function Harness({
  items,
  onAction,
}: {
  items: AppShortcutItem[];
  onAction: (item: AppShortcutItem) => void;
}) {
  const activePathRef = useRef<string | null>("note.md");
  useAppKeyboard({
    items,
    vaultPath: "vault",
    activePathRef,
    onAction,
  });
  return createElement("div", {
    "data-testid": "editor-dom",
    onKeyDown: (event) => event.stopPropagation(),
    tabIndex: 0,
  });
}

describe("useAppKeyboard", () => {
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

  it("handles Ctrl+Period before editor key handlers can stop propagation", () => {
    const toggleZen: AppShortcutItem = {
      id: "toggle-zen",
      chord: { key: ".", mod: true },
      action: { type: "toggleZen" },
    };
    const onAction = vi.fn();

    act(() => {
      root.render(createElement(Harness, { items: [toggleZen], onAction }));
    });

    const editorDom = container.querySelector<HTMLElement>(
      '[data-testid="editor-dom"]',
    );
    editorDom?.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: "Process",
        code: "Period",
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );

    expect(onAction).toHaveBeenCalledWith(toggleZen);
  });

  it("falls back to keyup when WebView misses the keydown shortcut", () => {
    const toggleZen: AppShortcutItem = {
      id: "toggle-zen",
      chord: { key: ".", mod: true },
      action: { type: "toggleZen" },
    };
    const onAction = vi.fn();

    act(() => {
      root.render(createElement(Harness, { items: [toggleZen], onAction }));
    });

    window.dispatchEvent(
      new KeyboardEvent("keyup", {
        key: "Process",
        code: "Period",
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );

    expect(onAction).toHaveBeenCalledWith(toggleZen);
  });

  it("does not toggle twice for one Ctrl+Period key press", () => {
    const toggleZen: AppShortcutItem = {
      id: "toggle-zen",
      chord: { key: ".", mod: true },
      action: { type: "toggleZen" },
    };
    const onAction = vi.fn();

    act(() => {
      root.render(createElement(Harness, { items: [toggleZen], onAction }));
    });

    const eventInit: KeyboardEventInit = {
      key: "Process",
      code: "Period",
      ctrlKey: true,
      bubbles: true,
      cancelable: true,
    };
    window.dispatchEvent(new KeyboardEvent("keydown", eventInit));
    window.dispatchEvent(new KeyboardEvent("keyup", eventInit));

    expect(onAction).toHaveBeenCalledTimes(1);
  });
});
