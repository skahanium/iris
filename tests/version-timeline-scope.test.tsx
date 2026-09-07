import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { VersionTimeline } from "@/components/version/VersionTimeline";
import type { VersionEntry } from "@/types/ipc";

const ipc = vi.hoisted(() => ({
  versionList: vi.fn(),
  versionPreview: vi.fn(),
  versionRestore: vi.fn(),
  versionDelete: vi.fn(),
}));
vi.mock("@/lib/ipc", () => ipc);

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
function version(id: number, label: string, legacy = false): VersionEntry {
  return {
    id,
    file_id: id,
    version_no: String(id),
    label,
    content_hash: "hash",
    word_count: 20,
    is_finalized: false,
    is_legacy_unscoped: legacy,
    kind: "manual",
    created_at: "2026-09-06T00:00:00Z",
  };
}

describe("version timeline document ownership", () => {
  let host: HTMLDivElement;
  let root: Root;
  const onRestore = vi.fn(async (_content: string) => {});
  let current = "current Markdown";
  beforeEach(() => {
    vi.clearAllMocks();
    current = "current Markdown";
    ipc.versionList.mockResolvedValue([version(1, "old version", true)]);
    ipc.versionPreview.mockResolvedValue("historical Markdown");
    ipc.versionRestore.mockResolvedValue({ content: "restored Markdown" });
    vi.spyOn(window, "confirm").mockReturnValue(true);
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });
  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.restoreAllMocks();
  });
  async function render(
    vaultPath = "/vault-A",
    notePath = "note.md",
    options: { withoutGetter?: boolean; session?: string; open?: boolean } = {},
  ) {
    await act(async () => {
      root.render(
        <VersionTimeline
          open={options.open ?? true}
          documentSessionId={options.session}
          onClose={() => {}}
          vaultPath={vaultPath}
          notePath={notePath}
          currentContent={current}
          getCurrentContent={options.withoutGetter ? undefined : () => current}
          onRestore={onRestore}
          onFinalizeCurrent={async () => null}
        />,
      );
    });
  }
  async function preview() {
    await act(async () => {
      (
        document.querySelector(
          '[data-testid="version-entry-row"]',
        ) as HTMLButtonElement
      ).click();
    });
  }
  async function restore() {
    await act(async () => {
      (
        document.querySelector(
          'button[title^="将当前正文"]',
        ) as HTMLButtonElement
      ).click();
    });
  }
  it("sends explicit ownership and confirmed legacy restoration to IPC", async () => {
    await render();
    await preview();
    await restore();
    expect(document.body.textContent).toContain("归属待确认");
    expect(window.confirm).toHaveBeenCalledWith(
      expect.stringContaining("note.md"),
    );
    expect(ipc.versionList).toHaveBeenCalledWith("note.md", "/vault-A");
    expect(ipc.versionPreview).toHaveBeenCalledWith(1, "/vault-A");
    expect(ipc.versionRestore).toHaveBeenCalledWith(1, "current Markdown", {
      targetPath: "note.md",
      expectedVault: "/vault-A",
      allowLegacyUnscoped: true,
    });
  });
  it("does not replace the new Vault's list with a late old list", async () => {
    const old = deferred<VersionEntry[]>();
    ipc.versionList
      .mockReturnValueOnce(old.promise)
      .mockResolvedValueOnce([version(2, "new vault version")]);
    await render();
    await render("/vault-B");
    await act(async () => old.resolve([version(1, "late old vault version")]));
    expect(document.body.textContent).toContain("new vault version");
    expect(document.body.textContent).not.toContain("late old vault version");
  });
  it("does not apply a late restore to a different note", async () => {
    const old = deferred<{ content: string }>();
    ipc.versionRestore.mockReturnValueOnce(old.promise);
    await render();
    await preview();
    await restore();
    await render("/vault-A", "other.md");
    await act(async () => old.resolve({ content: "wrong document" }));
    expect(onRestore).not.toHaveBeenCalled();
  });
  it("does not replace edits made while a restore snapshot is loading", async () => {
    const old = deferred<{ content: string }>();
    ipc.versionRestore.mockReturnValueOnce(old.promise);
    await render();
    await preview();
    await restore();
    current = "newer user edit";
    await act(async () => old.resolve({ content: "older snapshot" }));
    expect(onRestore).not.toHaveBeenCalled();
    expect(document.querySelector('[role="alert"]')?.textContent).toContain(
      "已发生变化",
    );
  });
  it("keeps a newer preview selected when an earlier preview arrives late", async () => {
    const old = deferred<string>();
    ipc.versionList.mockResolvedValueOnce([
      version(1, "first"),
      version(2, "second"),
    ]);
    ipc.versionPreview
      .mockReturnValueOnce(old.promise)
      .mockResolvedValueOnce("second preview");
    await render();
    const rows = document.querySelectorAll<HTMLButtonElement>(
      '[data-testid="version-entry-row"]',
    );
    expect(rows).toHaveLength(2);
    await act(async () => rows[0]!.click());
    await act(async () => rows[1]!.click());
    await act(async () => old.resolve("first preview"));
    expect(document.body.textContent).toContain("second preview");
    expect(document.body.textContent).not.toContain("first preview");
  });
  it("checks the latest prop when no live getter was supplied", async () => {
    const old = deferred<{ content: string }>();
    ipc.versionRestore.mockReturnValueOnce(old.promise);
    await render("/vault-A", "note.md", { withoutGetter: true });
    await preview();
    await restore();
    current = "edited while waiting";
    await render("/vault-A", "note.md", { withoutGetter: true });
    await act(async () => old.resolve({ content: "older snapshot" }));
    expect(onRestore).not.toHaveBeenCalled();
    expect(document.querySelector('[role="alert"]')?.textContent).toContain(
      "已发生变化",
    );
  });
  it.each(["session", "reopen"])(
    "does not apply a late restore after %s changes",
    async (change) => {
      const old = deferred<{ content: string }>();
      ipc.versionRestore.mockReturnValueOnce(old.promise);
      await render("/vault-A", "note.md", { session: "original" });
      await preview();
      await restore();
      if (change === "reopen") {
        await render("/vault-A", "note.md", {
          session: "original",
          open: false,
        });
      }
      await render("/vault-A", "note.md", {
        session: change === "session" ? "replacement" : "original",
      });
      await act(async () => old.resolve({ content: "old session content" }));
      expect(onRestore).not.toHaveBeenCalled();
    },
  );
  it("only includes unassigned legacy history after the explicit toggle", async () => {
    await render();
    expect(ipc.versionList).toHaveBeenLastCalledWith("note.md", "/vault-A");
    const toggle = [...document.querySelectorAll("button")].find(
      (button) => button.textContent === "查看未归属历史",
    );
    expect(toggle).toBeDefined();
    await act(async () => toggle!.click());
    expect(ipc.versionList).toHaveBeenLastCalledWith(
      "note.md",
      "/vault-A",
      true,
    );
    expect(toggle!.getAttribute("aria-pressed")).toBe("true");
    await act(async () => toggle!.click());
    expect(ipc.versionList).toHaveBeenLastCalledWith("note.md", "/vault-A");
  });
});
