import { readFileSync } from "node:fs";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { EditorZoomControl } from "@/components/layout/EditorZoomControl";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

let root: Root | null = null;
let host: HTMLDivElement | null = null;

function renderControl(zoom: number) {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  const handlers = {
    onZoomIn: vi.fn(),
    onZoomOut: vi.fn(),
    onZoomReset: vi.fn(),
    onZoomChange: vi.fn(),
  };

  act(() => {
    root?.render(
      <EditorZoomControl
        editorZoom={zoom}
        onZoomIn={handlers.onZoomIn}
        onZoomOut={handlers.onZoomOut}
        onZoomReset={handlers.onZoomReset}
        onZoomChange={handlers.onZoomChange}
      />,
    );
  });

  act(() => {
    document
      .querySelector<HTMLButtonElement>('[aria-label="编辑器缩放"]')
      ?.click();
  });

  return handlers;
}

afterEach(() => {
  if (root) {
    act(() => root?.unmount());
  }
  host?.remove();
  root = null;
  host = null;
});

describe("EditorZoomControl", () => {
  it("uses slider plus stepper buttons and disables zoom-in at maximum", () => {
    renderControl(1.5);

    expect(
      document.querySelector('input[type="range"][aria-label="缩放比例"]'),
    ).not.toBeNull();
    expect(document.body.textContent).toContain("150%");
    expect(
      document.querySelector<HTMLButtonElement>('[aria-label="放大"]')
        ?.disabled,
    ).toBe(true);
    expect(
      document.querySelector<HTMLButtonElement>('[aria-label="缩小"]')
        ?.disabled,
    ).toBe(false);
  });

  it("uses lucide controls rather than text-only zoom commands", () => {
    const source = read("src/components/layout/EditorZoomControl.tsx");
    expect(source).toContain("Plus");
    expect(source).toContain("Minus");
    expect(source).toContain("RotateCcw");
    expect(source).not.toMatch(/>\s*[-−+]\s*</);
    expect(source).not.toMatch(/>\s*重置\s*</);
  });
});
