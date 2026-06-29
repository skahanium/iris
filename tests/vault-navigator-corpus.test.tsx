import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { VaultNavigator } from "@/components/file/VaultNavigator";
import { createDefaultNote } from "@/lib/note-create";

const corpusList = vi.fn();
const corpusUpsert = vi.fn();
const fileDelete = vi.fn();
const fileList = vi.fn();
const fileRename = vi.fn();
const fileSetLock = vi.fn();
const folderList = vi.fn();
const folderCreate = vi.fn();
const folderRename = vi.fn();
const knowledgeReindex = vi.fn();
const templateList = vi.fn();
const prepareNoteOpenFromContent = vi.fn();

interface MockFileItem {
  path: string;
  title: string;
  updatedAt: string;
  isLocked: boolean;
}

vi.mock("@/lib/ipc", () => ({
  corpusList: (...args: unknown[]) => corpusList(...args),
  corpusUpsert: (...args: unknown[]) => corpusUpsert(...args),
  exportFile: vi.fn(),
  fileDelete: (...args: unknown[]) => fileDelete(...args),
  fileList: (...args: unknown[]) => fileList(...args),
  workspaceList: (...args: unknown[]) =>
    fileList(...args).then((items: MockFileItem[]) =>
      items.map((item) => ({
        attachmentRole: "formal",
        isLocked: item.isLocked,
        kind: "note",
        mediaKind: null,
        mimeType: null,
        path: item.path,
        sizeBytes: null,
        title: item.title,
        updatedAt: item.updatedAt,
      })),
    ),
  fileRead: vi.fn(),
  fileRename: (...args: unknown[]) => fileRename(...args),
  fileSetLock: (...args: unknown[]) => fileSetLock(...args),
  folderCreate: (...args: unknown[]) => folderCreate(...args),
  folderDelete: vi.fn(),
  folderList: (...args: unknown[]) => folderList(...args),
  folderRename: (...args: unknown[]) => folderRename(...args),
  knowledgeReindex: (...args: unknown[]) => knowledgeReindex(...args),
  templateCreate: vi.fn(),
  templateList: (...args: unknown[]) => templateList(...args),
}));

vi.mock("@/lib/note-create", () => ({
  createDefaultNote: vi.fn(),
}));

vi.mock("@/lib/note-open-preparation", async () => {
  const actual = await vi.importActual<
    typeof import("@/lib/note-open-preparation")
  >("@/lib/note-open-preparation");
  return {
    ...actual,
    prepareNoteOpenFromContent: (...args: unknown[]) =>
      prepareNoteOpenFromContent(...args),
  };
});

describe("VaultNavigator corpus assignment", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    corpusList.mockReset();
    corpusUpsert.mockReset();
    fileDelete.mockReset();
    fileList.mockReset();
    fileRename.mockReset();
    fileSetLock.mockReset();
    folderCreate.mockReset();
    folderList.mockReset();
    folderRename.mockReset();
    knowledgeReindex.mockReset();
    templateList.mockReset();
    prepareNoteOpenFromContent.mockReset();
    vi.mocked(createDefaultNote).mockReset();
    fileList.mockResolvedValue([
      {
        path: "policy/a.md",
        title: "A",
        updatedAt: "",
        isLocked: false,
      },
      {
        path: "policy/b.md",
        title: "B",
        updatedAt: "",
        isLocked: false,
      },
    ]);
    folderList.mockResolvedValue(["policy/", "archive/"]);
    corpusList.mockResolvedValue([]);
    templateList.mockResolvedValue([]);
    corpusUpsert.mockResolvedValue(undefined);
    fileDelete.mockResolvedValue(undefined);
    fileRename.mockResolvedValue(undefined);
    fileSetLock.mockResolvedValue(undefined);
    folderCreate.mockResolvedValue(undefined);
    folderRename.mockResolvedValue(undefined);
    knowledgeReindex.mockResolvedValue({ anchors: 0, regulations: 1 });
    vi.mocked(createDefaultNote).mockResolvedValue({
      content: '---\ntitle: "未命名文档"\n---\n\n',
      path: "未命名文档.md",
      title: "未命名文档",
    });
    prepareNoteOpenFromContent.mockImplementation(
      async (
        request: { path: string; titleHint?: string },
        source: { content: string; isLocked: boolean },
      ) => ({
        bodyMarkdown: "\n",
        content: source.content,
        frontmatterYaml: 'title: "未命名文档"',
        isLocked: source.isLocked,
        namespace: "normal",
        path: request.path,
        signature: "prepared-file-tree-new-note",
        title: request.titleHint ?? "未命名文档",
        traceKey: "trace-file-tree-new-note",
      }),
    );
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  async function renderNavigator() {
    await act(async () => {
      root.render(<VaultNavigator open onClose={vi.fn()} onOpen={vi.fn()} />);
    });
    await vi.waitFor(() => {
      expect(document.body.textContent).toContain("policy");
    });
  }

  async function selectPolicyFolder() {
    const folderButton = Array.from(document.querySelectorAll("button")).find(
      (button) => button.textContent?.includes("policy"),
    );
    expect(folderButton).toBeTruthy();
    await act(async () => {
      folderButton?.click();
    });
  }

  function setInputValue(input: HTMLInputElement, value: string) {
    act(() => {
      const setter = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        "value",
      )?.set;
      setter?.call(input, value);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
  }

  function findButton(label: string): HTMLButtonElement {
    const button = Array.from(document.querySelectorAll("button")).find(
      (candidate) =>
        candidate.textContent?.includes(label) ||
        candidate.getAttribute("title")?.includes(label),
    );
    if (!button) throw new Error(`button missing: ${label}`);
    return button;
  }

  function corpusConfirmButton(): HTMLButtonElement {
    const button = document.querySelector<HTMLButtonElement>(
      '[data-testid="corpus-confirm-button"]',
    );
    if (!button) throw new Error("corpus confirm button missing");
    return button;
  }

  function fileActionButton(label: string): HTMLButtonElement {
    const button = document.querySelector<HTMLButtonElement>(
      `button[aria-label="${label}"]`,
    );
    if (!button) throw new Error(`file action missing: ${label}`);
    return button;
  }
  function newNoteInput(): HTMLInputElement {
    const input = document.querySelector<HTMLInputElement>(
      ".task-overlay-filter input",
    );
    if (!input) throw new Error("new note input missing");
    return input;
  }

  it("keeps the new-note field empty and uses default allocation for empty creates", async () => {
    const onOpen = vi.fn();
    await act(async () => {
      root.render(<VaultNavigator open onClose={vi.fn()} onOpen={onOpen} />);
    });
    await vi.waitFor(() => {
      expect(document.body.textContent).toContain("policy");
    });

    const input = newNoteInput();
    expect(input.value).toBe("");
    expect(input.placeholder).toBe("未命名文档.md");

    await act(async () => {
      document
        .querySelector<HTMLButtonElement>('button[title="新建笔记"]')
        ?.click();
      await Promise.resolve();
    });

    expect(createDefaultNote).toHaveBeenCalledWith({ folderPrefix: "" });
    expect(prepareNoteOpenFromContent).toHaveBeenCalledWith(
      expect.objectContaining({
        path: "未命名文档.md",
        priority: "hot",
        source: "new-note",
        titleHint: "未命名文档",
      }),
      {
        content: '---\ntitle: "未命名文档"\n---\n\n',
        isLocked: false,
      },
    );
    expect(onOpen).toHaveBeenCalledWith(
      "未命名文档.md",
      "file-tree",
      expect.objectContaining({
        openBudgetKind: "hot",
        preparedNote: expect.objectContaining({
          path: "未命名文档.md",
          title: "未命名文档",
        }),
      }),
    );
    expect(newNoteInput().value).toBe("");
  });

  it("creates folders from a dedicated dialog with parent and path preview", async () => {
    await renderNavigator();
    await selectPolicyFolder();

    await act(async () => {
      findButton("新建文件夹").click();
    });

    expect(document.body.textContent).toContain("父级位置");
    expect(document.body.textContent).toContain("最终路径");
    expect(document.body.textContent).toContain("policy/");

    const input = document.querySelector<HTMLInputElement>(
      'input[aria-label="文件夹名称"]',
    );
    expect(input).toBeTruthy();
    setInputValue(input!, "drafts");
    expect(document.body.textContent).toContain("policy/drafts/");

    await act(async () => {
      findButton("创建文件夹").click();
    });

    expect(folderCreate).toHaveBeenCalledWith("policy/drafts");
  });

  it("shows corpus choices in the selected folder details and reindexes after confirming", async () => {
    await renderNavigator();
    await selectPolicyFolder();

    const details = document.querySelector<HTMLElement>(
      '[data-testid="folder-details"]',
    );
    expect(details?.getAttribute("data-density")).toBe("compact");
    const corpusSelect = document.querySelector<HTMLElement>(
      '[data-testid="corpus-kind-select"]',
    );
    expect(corpusSelect?.getAttribute("data-layout")).toBe("dropdown");
    expect(
      document.querySelector('[data-testid="corpus-kind-options"]'),
    ).toBeNull();
    expect(document.body.textContent).toContain("文件夹详情");
    expect(document.body.textContent).toContain("语料库类型");
    expect(document.body.textContent).toContain("规范依据");
    expect(document.body.textContent).not.toContain(
      "选择这个文件夹在 AI 检索中的用途。",
    );
    expect(document.body.textContent).not.toContain("AI 必须优先遵循");
    const corpusTrigger = document.querySelector<HTMLButtonElement>(
      'button[aria-label="语料库类型"]',
    );
    expect(corpusTrigger?.getAttribute("title")).toContain("AI 必须优先遵循");
    expect(document.body.textContent).toContain("policy/");
    expect(document.body.textContent).not.toContain(
      "regulation / exemplar / general",
    );

    await act(async () => {
      corpusConfirmButton().click();
    });

    expect(corpusUpsert).toHaveBeenCalledWith({
      id: "policy",
      name: "policy",
      pathPrefix: "policy/",
      kind: "authority",
      scenes: ["knowledge_lookup", "research_synthesis", "drafting_assist"],
    });
    expect(knowledgeReindex).toHaveBeenCalledOnce();
  });

  it("hydrates saved corpus role by folder path before confirming", async () => {
    corpusList.mockResolvedValue([
      {
        id: "policy",
        name: "policy",
        pathPrefix: "policy/",
        kind: "exemplar",
        scenes: ["exemplar_learning", "drafting_assist"],
      },
    ]);

    await renderNavigator();
    await selectPolicyFolder();

    await act(async () => {
      corpusConfirmButton().click();
    });

    expect(corpusUpsert).toHaveBeenCalledWith({
      id: "policy",
      name: "policy",
      pathPrefix: "policy/",
      kind: "exemplar",
      scenes: ["drafting_assist"],
    });
  });

  it("renames a document by name within its current folder", async () => {
    await renderNavigator();
    await selectPolicyFolder();

    await act(async () => {
      findButton("重命名文档").click();
    });

    const input = document.querySelector<HTMLInputElement>(
      'input[aria-label="文档名称"]',
    );
    expect(input).toBeTruthy();
    setInputValue(input!, "b.md");

    await act(async () => {
      findButton("保存名称").click();
    });

    expect(fileRename).toHaveBeenCalledWith("policy/a.md", "policy/b.md");
  });

  it("moves a document by choosing a target folder", async () => {
    await renderNavigator();
    await selectPolicyFolder();

    await act(async () => {
      findButton("移动文档").click();
    });

    expect(document.body.textContent).toContain("选择目标文件夹");
    await act(async () => {
      findButton("archive/").click();
    });
    await act(async () => {
      findButton("移动到此处").click();
    });

    expect(fileRename).toHaveBeenCalledWith("policy/a.md", "archive/a.md");
  });

  it("surfaces structured move errors instead of hiding them behind a generic label", async () => {
    fileRename.mockRejectedValueOnce({ message: "IO error" });
    await renderNavigator();
    await selectPolicyFolder();

    await act(async () => {
      fileActionButton("移动文档").click();
    });
    await act(async () => {
      findButton("archive/").click();
    });
    await act(async () => {
      findButton("移动到此处").click();
    });

    expect(document.body.textContent).toContain("IO error");
    expect(document.body.textContent).not.toContain("移动失败");
  });

  it("moves placeholder-named documents with their real display title", async () => {
    fileList.mockResolvedValue([
      {
        path: "policy/未命名文档.md",
        title: "案件总结",
        updatedAt: "",
        isLocked: false,
      },
    ]);
    await renderNavigator();
    await selectPolicyFolder();

    await act(async () => {
      fileActionButton("移动文档").click();
    });
    await act(async () => {
      findButton("archive/").click();
    });
    expect(document.body.textContent).toContain("archive/案件总结.md");
    await act(async () => {
      findButton("移动到此处").click();
    });

    expect(fileRename).toHaveBeenCalledWith(
      "policy/未命名文档.md",
      "archive/案件总结.md",
    );
  });

  it("preserves custom file basenames when moving documents with different titles", async () => {
    fileList.mockResolvedValue([
      {
        path: "policy/custom-slug.md",
        title: "案件总结",
        updatedAt: "",
        isLocked: false,
      },
    ]);
    await renderNavigator();
    await selectPolicyFolder();

    await act(async () => {
      fileActionButton("移动文档").click();
    });
    await act(async () => {
      findButton("archive/").click();
    });
    await act(async () => {
      findButton("移动到此处").click();
    });

    expect(fileRename).toHaveBeenCalledWith(
      "policy/custom-slug.md",
      "archive/custom-slug.md",
    );
  });

  it("allocates a suffix when a title-based move target already exists", async () => {
    fileList.mockResolvedValue([
      {
        path: "policy/未命名文档.md",
        title: "案件总结",
        updatedAt: "",
        isLocked: false,
      },
      {
        path: "archive/案件总结.md",
        title: "案件总结",
        updatedAt: "",
        isLocked: false,
      },
    ]);
    await renderNavigator();
    await selectPolicyFolder();

    await act(async () => {
      fileActionButton("移动文档").click();
    });
    await act(async () => {
      findButton("archive/").click();
    });
    await act(async () => {
      findButton("移动到此处").click();
    });

    expect(fileRename).toHaveBeenCalledWith(
      "policy/未命名文档.md",
      "archive/案件总结（1）.md",
    );
  });

  it("uses icon-only buttons for document rename and move", async () => {
    await renderNavigator();
    await selectPolicyFolder();

    const renameButton = document.querySelector<HTMLButtonElement>(
      'button[title="重命名文档"]',
    );
    const moveButton = document.querySelector<HTMLButtonElement>(
      'button[title="移动文档"]',
    );

    expect(renameButton).toBeTruthy();
    expect(moveButton).toBeTruthy();
    expect(renameButton?.getAttribute("aria-label")).toBe("重命名文档");
    expect(moveButton?.getAttribute("aria-label")).toBe("移动文档");
    expect(renameButton?.textContent?.trim()).toBe("");
    expect(moveButton?.textContent?.trim()).toBe("");
  });

  it("batch moves, locks, unlocks, and deletes selected documents", async () => {
    await renderNavigator();
    await selectPolicyFolder();

    await act(async () => {
      findButton("批量操作").click();
    });

    const checkboxes = Array.from(
      document.querySelectorAll<HTMLInputElement>(
        'input[type="checkbox"][aria-label^="选择文档"]',
      ),
    );
    expect(checkboxes).toHaveLength(2);
    await act(async () => {
      checkboxes[0]!.click();
      checkboxes[1]!.click();
    });
    expect(document.body.textContent).toContain("已选 2 个文档");

    await act(async () => {
      findButton("批量移动").click();
    });
    await act(async () => {
      findButton("archive/").click();
    });
    await act(async () => {
      findButton("移动到此处").click();
    });
    expect(fileRename).toHaveBeenCalledWith("policy/a.md", "archive/a.md");
    expect(fileRename).toHaveBeenCalledWith("policy/b.md", "archive/b.md");

    await act(async () => {
      findButton("批量锁定").click();
    });
    expect(fileSetLock).toHaveBeenCalledWith("policy/a.md", true);
    expect(fileSetLock).toHaveBeenCalledWith("policy/b.md", true);

    await act(async () => {
      findButton("批量解锁").click();
    });
    expect(fileSetLock).toHaveBeenCalledWith("policy/a.md", false);
    expect(fileSetLock).toHaveBeenCalledWith("policy/b.md", false);

    await act(async () => {
      findButton("批量删除").click();
    });
    await act(async () => {
      findButton("删除").click();
    });
    expect(fileDelete).toHaveBeenCalledWith("policy/a.md");
    expect(fileDelete).toHaveBeenCalledWith("policy/b.md");
  });

  it("moves the selected folder by choosing a target parent", async () => {
    await renderNavigator();
    await selectPolicyFolder();

    await act(async () => {
      findButton("移动文件夹").click();
    });

    await act(async () => {
      findButton("archive/").click();
    });
    await act(async () => {
      findButton("移动到此处").click();
    });

    expect(folderRename).toHaveBeenCalledWith("policy/", "archive/policy");
  });

  it("prepares visible files and closes immediately when opening a file", async () => {
    const onPrepare = vi.fn();
    let resolveOpen!: () => void;
    const onOpen = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveOpen = resolve;
        }),
    );
    const onClose = vi.fn();

    await act(async () => {
      root.render(
        <VaultNavigator
          open
          onClose={onClose}
          onOpen={onOpen}
          onPrepare={onPrepare}
        />,
      );
    });
    await vi.waitFor(() => {
      expect(document.body.textContent).toContain("A");
    });
    expect(onPrepare).toHaveBeenCalledWith(
      {
        path: "policy/a.md",
        title: "A",
        updatedAt: "",
        isLocked: false,
      },
      "file-tree",
    );

    const fileButton = Array.from(document.querySelectorAll("button")).find(
      (button) => button.textContent?.trim() === "A",
    );
    expect(fileButton).toBeTruthy();
    await act(async () => {
      fileButton?.click();
      await Promise.resolve();
    });
    expect(onOpen).toHaveBeenCalledWith("policy/a.md", "file-tree");
    expect(onClose).toHaveBeenCalledOnce();

    await act(async () => {
      resolveOpen();
      await Promise.resolve();
    });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("prepares only the first eight visible note candidates from the file tree", async () => {
    const onPrepare = vi.fn();
    fileList.mockResolvedValue(
      Array.from({ length: 12 }, (_, index) => ({
        path: `notes/${index}.md`,
        title: `Note ${index}`,
        updatedAt: "",
        isLocked: false,
      })),
    );

    await act(async () => {
      root.render(
        <VaultNavigator
          open
          onClose={vi.fn()}
          onOpen={vi.fn()}
          onPrepare={onPrepare}
        />,
      );
    });

    await vi.waitFor(() => expect(onPrepare).toHaveBeenCalledTimes(8));
    expect(onPrepare.mock.calls.map(([file]) => file.path)).toEqual([
      "notes/0.md",
      "notes/1.md",
      "notes/2.md",
      "notes/3.md",
      "notes/4.md",
      "notes/5.md",
      "notes/6.md",
      "notes/7.md",
    ]);
  });

  it("does not expose HTML export in the file row", async () => {
    await renderNavigator();
    await selectPolicyFolder();

    expect(document.body.textContent).not.toContain("导出 HTML");
    expect(document.querySelector('button[title="导出 HTML"]')).toBeNull();
  });
});
