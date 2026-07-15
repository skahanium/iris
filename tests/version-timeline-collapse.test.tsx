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

function manualEntry(id = 1): VersionEntry {
  return {
    ...autoEntry(id),
    kind: "manual",
    version_no: "20260526143000000",
    created_at: "2026-05-26T14:30:00Z",
  };
}

const VersionTimelineHarness = VersionTimeline as unknown as (props: {
  open: boolean;
  onClose: () => void;
  notePath: string | null;
  currentContent?: string;
  getCurrentContent?: () => string;
  hasUnsavedEdits?: boolean;
  onRestore: (content: string) => Promise<void>;
}) => ReturnType<typeof VersionTimeline>;

describe("VersionTimeline collapsed auto backups", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    versionList.mockReset();
    versionPreview.mockReset();
    versionRestore.mockReset();
    versionDelete.mockReset();
    versionFinalizeCurrent.mockReset();
    versionPreview.mockResolvedValue("# history");
    versionRestore.mockResolvedValue({ content: "# restored" });
    versionFinalizeCurrent.mockResolvedValue(null);
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
    vi.restoreAllMocks();
  });

  it("renders collapsed auto backup header without five row buttons", async () => {
    await act(async () => {
      root.render(
        createElement(VersionTimeline, {
          open: true,
          onClose: () => {},
          notePath: "note.md",
          currentContent: "# current",
          onRestore: async () => {},
        }),
      );
      await Promise.resolve();
    });

    expect(
      document.body.querySelector('[data-testid="version-group-toggle"]'),
    ).toBeTruthy();
    expect(
      document.body.querySelectorAll('[data-testid="version-entry-row"]'),
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
          onRestore: async () => {},
        }),
      );
      await Promise.resolve();
    });

    const toggle = document.body.querySelector(
      '[data-testid="version-group-toggle"]',
    );
    expect(toggle).toBeTruthy();

    await act(async () => {
      (toggle as HTMLButtonElement).click();
    });

    expect(
      document.body.querySelectorAll('[data-testid="version-entry-row"]'),
    ).toHaveLength(5);
  });

  it("reads live current content only when restoring a version", async () => {
    versionList.mockResolvedValueOnce([manualEntry()]);
    const getCurrentContent = vi.fn(() => "# fresh current");
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    const onRestore = vi.fn(async () => {});

    await act(async () => {
      root.render(
        createElement(VersionTimelineHarness, {
          open: true,
          onClose: () => {},
          notePath: "note.md",
          currentContent: "# stale current",
          getCurrentContent,
          onRestore,
        }),
      );
      await Promise.resolve();
    });

    expect(getCurrentContent).not.toHaveBeenCalled();

    const row = document.body.querySelector(
      '[data-testid="version-entry-row"]',
    ) as HTMLButtonElement;
    await act(async () => {
      row.click();
      await Promise.resolve();
    });

    const restore = document.body.querySelector(
      "button[title]",
    ) as HTMLButtonElement;
    await act(async () => {
      restore.click();
      await Promise.resolve();
    });

    expect(confirm).toHaveBeenCalled();
    expect(getCurrentContent).toHaveBeenCalledTimes(1);
    expect(versionRestore).toHaveBeenCalledWith(1, "# fresh current");
    expect(onRestore).toHaveBeenCalledWith("# restored");
  });

  it("reads live current content only when finalizing the current note", async () => {
    const getCurrentContent = vi.fn(() => "# fresh current");

    await act(async () => {
      root.render(
        createElement(VersionTimelineHarness, {
          open: true,
          onClose: () => {},
          notePath: "note.md",
          currentContent: "# stale current",
          getCurrentContent,
          onRestore: async () => {},
        }),
      );
      await Promise.resolve();
    });

    expect(getCurrentContent).not.toHaveBeenCalled();

    const finalizeLabelInput = document.body.querySelector("input");
    const finalize = finalizeLabelInput?.nextElementSibling as
      | HTMLButtonElement
      | undefined;
    await act(async () => {
      finalize?.click();
      await Promise.resolve();
    });

    expect(getCurrentContent).toHaveBeenCalledTimes(1);
    expect(versionFinalizeCurrent).toHaveBeenCalledWith(
      "note.md",
      "# fresh current",
      null,
    );
  });
});
