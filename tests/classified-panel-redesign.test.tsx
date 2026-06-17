import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ClassifiedPanel } from "@/components/classified/ClassifiedPanel";
import type { ClassifiedStatus } from "@/types/ipc";

const classifiedFiles = vi.fn();
const classifiedMkdir = vi.fn();
let root: Root | null = null;

vi.mock("@/lib/ipc", async () => {
  const actual = await vi.importActual<typeof import("@/lib/ipc")>("@/lib/ipc");
  return {
    ...actual,
    classifiedFiles: (...args: unknown[]) => classifiedFiles(...args),
    classifiedMkdir: (...args: unknown[]) => classifiedMkdir(...args),
  };
});

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

function renderPanel(
  props: Partial<React.ComponentProps<typeof ClassifiedPanel>> = {},
) {
  const onClose = props.onClose ?? vi.fn();
  const rootProps: React.ComponentProps<typeof ClassifiedPanel> = {
    open: true,
    onClose,
    status: "unlocked",
    waiting: false,
    idleDeadline: null,
    openClassifiedPaths: [],
    onOpenFile: vi.fn(),
    onUnlockSuccess: vi.fn(),
    onRequestLock: vi.fn(async () => true),
    onActivity: vi.fn(),
    onRefreshStatus: vi.fn(async (): Promise<ClassifiedStatus> => "unlocked"),
    onEnterWaiting: vi.fn(),
    ...props,
  };

  act(() => {
    root?.render(<ClassifiedPanel {...rootProps} />);
  });
  return rootProps;
}

describe("ClassifiedPanel redesigned UI", () => {
  let host: HTMLDivElement;
  beforeEach(() => {
    classifiedFiles.mockReset();
    classifiedMkdir.mockReset();
    classifiedFiles.mockResolvedValue([
      { path: ".classified/inbox", isDir: true },
      { path: ".classified/secret.md", isDir: false },
    ]);
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root?.unmount());
    host.remove();
    root = null;
    vi.restoreAllMocks();
  });

  it("renders unlocked vault files without exposing the internal classified path", async () => {
    renderPanel();

    await vi.waitFor(() => {
      expect(document.body.textContent).toContain("secret.md");
    });

    expect(document.body.textContent).toContain("保险库");
    expect(document.body.textContent).toContain("已解锁");
    expect(document.body.textContent).not.toContain(".classified");
  });

  it("shows waiting state by count and file names instead of full internal paths", () => {
    renderPanel({
      waiting: true,
      status: "unlocked",
      openClassifiedPaths: [
        ".classified/private/a.md",
        ".classified/private/b.md",
      ],
    });

    expect(document.body.textContent).toContain("还有 2 个涉密标签页未关闭");
    expect(document.body.textContent).toContain("a.md");
    expect(document.body.textContent).toContain("b.md");
    expect(document.body.textContent).not.toContain(".classified");
    expect(document.body.textContent).not.toContain("private/");
  });

  it("uses Iris dialogs for folder creation instead of native prompts", async () => {
    const promptSpy = vi.spyOn(window, "prompt");

    renderPanel();
    await vi.waitFor(() => {
      expect(document.body.textContent).toContain("新建文件夹");
    });

    const folderButton = Array.from(document.querySelectorAll("button")).find(
      (button) => button.textContent?.includes("新建文件夹"),
    );
    expect(folderButton).toBeTruthy();

    await act(async () => {
      folderButton?.click();
    });

    expect(promptSpy).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain("文件夹名称");
  });
});
