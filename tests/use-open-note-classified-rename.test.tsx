import { act, createElement, useRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useOpenNote } from "@/hooks/useOpenNote";

const { classifiedRename, documentRenameByTitle } = vi.hoisted(() => ({
  classifiedRename: vi.fn(),
  documentRenameByTitle: vi.fn(),
}));

vi.mock("@/lib/ipc", () => ({
  classifiedRename,
  documentRenameByTitle,
}));

interface HookApi {
  noteTitle: string;
  onTitleBlur: (title?: string) => void;
  setTitleFocused: (focused: boolean) => void;
}

type ReplaceOpenTabPath = (
  oldPath: string,
  newPath: string,
  title?: string,
  markdownOverride?: string,
) => void;

function Harness({
  activePath,
  replaceOpenTabPath,
  onPathRenameError,
  onReady,
}: {
  activePath: { current: string | null };
  replaceOpenTabPath: ReplaceOpenTabPath;
  onPathRenameError?: () => void;
  onReady: (api: HookApi) => void;
}) {
  const activePathRef = useRef(activePath.current);
  activePathRef.current = activePath.current;
  const api = useOpenNote({
    activePath: activePath.current,
    editorContentTick: 1,
    activePathRef,
    markdownRef: { current: "# body\n" },
    frontmatterYamlRef: { current: null },
    editorRef: { current: null },
    updateTabTitle: vi.fn(),
    replaceOpenTabPath: (oldPath, newPath, title, markdownOverride) => {
      if (activePath.current === oldPath) {
        activePath.current = newPath;
        activePathRef.current = newPath;
      }
      replaceOpenTabPath(oldPath, newPath, title, markdownOverride);
    },
    onPathRenameError,
  });
  onReady({
    noteTitle: api.noteTitle,
    onTitleBlur: api.onTitleBlur,
    setTitleFocused: api.setTitleFocused,
  });
  return createElement("span", { "data-testid": "note-title" }, api.noteTitle);
}

describe("useOpenNote classified vault title rename", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    classifiedRename.mockReset();
    documentRenameByTitle.mockReset();
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  async function renderHarness(options: {
    activePath: string;
    replaceOpenTabPath?: ReplaceOpenTabPath;
    onPathRenameError?: () => void;
  }) {
    const activePath = { current: options.activePath as string | null };
    let api!: HookApi;
    const replaceOpenTabPath = vi.fn<ReplaceOpenTabPath>();
    const onPathRenameError = vi.fn();
    await act(async () => {
      root.render(
        createElement(Harness, {
          activePath,
          replaceOpenTabPath: options.replaceOpenTabPath ?? replaceOpenTabPath,
          onPathRenameError: options.onPathRenameError ?? onPathRenameError,
          onReady: (next) => {
            api = next;
          },
        }),
      );
    });
    return {
      activePath,
      api,
      noteTitleText: () =>
        host.querySelector('[data-testid="note-title"]')?.textContent ?? null,
      replaceOpenTabPath: options.replaceOpenTabPath ?? replaceOpenTabPath,
      onPathRenameError: options.onPathRenameError ?? onPathRenameError,
    };
  }

  it("renames a classified note through classifiedRename and never calls documentRenameByTitle", async () => {
    classifiedRename.mockResolvedValue(undefined);
    const { activePath, api, noteTitleText, replaceOpenTabPath } =
      await renderHarness({
        activePath: ".classified/旧名.md",
      });

    await act(async () => {
      api.onTitleBlur("新名");
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(classifiedRename).toHaveBeenCalledWith(
      ".classified/旧名.md",
      ".classified/新名.md",
    );
    expect(documentRenameByTitle).not.toHaveBeenCalled();
    expect(replaceOpenTabPath).toHaveBeenCalledWith(
      ".classified/旧名.md",
      ".classified/新名.md",
      "新名",
      "# body\n",
    );
    expect(activePath.current).toBe(".classified/新名.md");
    expect(noteTitleText()).toBe("新名");
  });

  it("computes the new path inside a nested classified folder", async () => {
    classifiedRename.mockResolvedValue(undefined);
    const { api } = await renderHarness({
      activePath: ".classified/inbox/旧名.md",
    });

    await act(async () => {
      api.onTitleBlur("新名");
    });
    await vi.waitFor(() => {
      expect(classifiedRename).toHaveBeenCalledWith(
        ".classified/inbox/旧名.md",
        ".classified/inbox/新名.md",
      );
    });
    expect(documentRenameByTitle).not.toHaveBeenCalled();
  });

  it("restores the original title when classifiedRename fails", async () => {
    classifiedRename.mockRejectedValue(new Error("classified rename denied"));
    const {
      activePath,
      api,
      noteTitleText,
      replaceOpenTabPath,
      onPathRenameError,
    } = await renderHarness({
      activePath: ".classified/旧名.md",
    });

    await act(async () => {
      api.onTitleBlur("新名");
    });
    await vi.waitFor(() => {
      expect(onPathRenameError).toHaveBeenCalledTimes(1);
    });
    expect(noteTitleText()).toBe("旧名");
    expect(activePath.current).toBe(".classified/旧名.md");
    expect(replaceOpenTabPath).not.toHaveBeenCalled();
    expect(documentRenameByTitle).not.toHaveBeenCalled();
  });

  it("keeps normal notes on documentRenameByTitle and does not call classifiedRename", async () => {
    documentRenameByTitle.mockResolvedValue({
      entry: { path: "新名.md" },
      indexStatus: "synced",
    });
    const { activePath, api, noteTitleText, replaceOpenTabPath } =
      await renderHarness({
        activePath: "旧名.md",
      });

    await act(async () => {
      api.onTitleBlur("新名");
    });
    await vi.waitFor(() => {
      expect(documentRenameByTitle).toHaveBeenCalledWith("旧名.md", "新名");
    });
    expect(classifiedRename).not.toHaveBeenCalled();
    expect(activePath.current).toBe("新名.md");
    expect(noteTitleText()).toBe("新名");
    expect(replaceOpenTabPath).toHaveBeenCalledWith(
      "旧名.md",
      "新名.md",
      "新名",
      "# body\n",
    );
  });

  it("does not call rename IPC for an empty classified title", async () => {
    const { api, noteTitleText, replaceOpenTabPath, onPathRenameError } =
      await renderHarness({
        activePath: ".classified/旧名.md",
      });

    await act(async () => {
      api.onTitleBlur("");
    });
    await vi.waitFor(() => {
      expect(noteTitleText()).toBe("旧名");
    });
    expect(classifiedRename).not.toHaveBeenCalled();
    expect(documentRenameByTitle).not.toHaveBeenCalled();
    expect(replaceOpenTabPath).not.toHaveBeenCalled();
    expect(onPathRenameError).not.toHaveBeenCalled();
  });
});
