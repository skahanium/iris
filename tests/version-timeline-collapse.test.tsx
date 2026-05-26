import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { VersionTimeline } from "@/components/version/VersionTimeline";
import type { VersionEntry } from "@/types/ipc";

const versionList = vi.fn();
const versionPreview = vi.fn();
const versionRestore = vi.fn();
const versionDelete = vi.fn();
const versionFinalizeCurrent = vi.fn();

vi.mock("@/lib/ipc", () => ({
  versionList: (...args: unknown[]) => versionList(...args),
  versionPreview: (...args: unknown[]) => versionPreview(...args),
  versionRestore: (...args: unknown[]) => versionRestore(...args),
  versionDelete: (...args: unknown[]) => versionDelete(...args),
  versionFinalizeCurrent: (...args: unknown[]) =>
    versionFinalizeCurrent(...args),
}));

function autoEntry(id: number): VersionEntry {
  return {
    id,
    file_id: 1,
    version_no: `20260526120${String(id).padStart(5, "0")}`,
    label: null,
    content_hash: `h${id}`,
    word_count: 10,
    is_finalized: false,
    kind: "auto_idle",
    created_at: `2026-05-26T12:00:${String(id).padStart(2, "0")}Z`,
  };
}

describe("VersionTimeline collapsed auto backups", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    versionList.mockReset();
    versionPreview.mockReset();
    versionList.mockResolvedValue(
      Array.from({ length: 5 }, (_, i) => autoEntry(i + 1)),
    );
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

  it("renders collapsed auto backup header without five row buttons", async () => {
    await act(async () => {
      root.render(
        createElement(VersionTimeline, {
          open: true,
          onClose: () => {},
          notePath: "note.md",
          currentContent: "# current",
          onRestore: () => {},
        }),
      );
      await Promise.resolve();
    });

    expect(container.textContent).toContain("自动备份（5）");
    expect(
      container.querySelectorAll('[data-testid="version-entry-row"]'),
    ).toHaveLength(0);
  });

  it("expands auto backup rows when header is clicked", async () => {
    await act(async () => {
      root.render(
        createElement(VersionTimeline, {
          open: true,
          onClose: () => {},
          notePath: "note.md",
          currentContent: "# current",
          onRestore: () => {},
        }),
      );
      await Promise.resolve();
    });

    const toggle = container.querySelector('[data-testid="version-group-toggle"]');
    expect(toggle).toBeTruthy();

    await act(async () => {
      (toggle as HTMLButtonElement).click();
    });

    expect(
      container.querySelectorAll('[data-testid="version-entry-row"]'),
    ).toHaveLength(5);
  });
});
