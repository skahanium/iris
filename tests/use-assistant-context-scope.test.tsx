import { act, createElement, type RefObject } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useAssistantContextScope } from "@/components/ai/hooks/useAssistantContextScope";
import type { FileListItem, TagGroup } from "@/types/ipc";

const files: FileListItem[] = [
  {
    path: "Policies/Guide.md",
    title: "Guide",
    updatedAt: "2026-01-01",
    isLocked: false,
  },
  {
    path: "Research/Notes/Alpha.md",
    title: "Alpha",
    updatedAt: "2026-01-01",
    isLocked: false,
  },
];

type HookApi = ReturnType<typeof useAssistantContextScope>;

function Harness({
  input,
  loadVaultFiles = async () => files,
  loadVaultTags = async () => [],
  runtimeDocumentCandidates = [],
  onInput,
  onReady,
  textareaRef,
}: {
  input: string;
  loadVaultFiles?: () => Promise<FileListItem[]>;
  loadVaultTags?: () => Promise<TagGroup[]>;
  runtimeDocumentCandidates?: FileListItem[];
  onInput: (next: string | ((prev: string) => string)) => void;
  onReady: (api: HookApi) => void;
  textareaRef: RefObject<HTMLTextAreaElement | null>;
}) {
  const api = useAssistantContextScope({
    input,
    setInput: onInput,
    textareaRef,
    loadVaultFiles,
    loadVaultTags,
    runtimeDocumentCandidates,
  });
  onReady(api);
  return null;
}

describe("useAssistantContextScope", () => {
  let container: HTMLDivElement;
  let root: Root;
  let textarea: HTMLTextAreaElement;
  let input: string;
  let api!: HookApi;
  let textareaRef: RefObject<HTMLTextAreaElement | null>;
  let loadVaultFiles: () => Promise<FileListItem[]>;
  let loadVaultTags: () => Promise<TagGroup[]>;
  let runtimeDocumentCandidates: FileListItem[];

  function setInput(next: string | ((prev: string) => string)) {
    input = typeof next === "function" ? next(input) : next;
    render();
  }

  function render() {
    root.render(
      createElement(Harness, {
        input,
        loadVaultFiles,
        loadVaultTags,
        runtimeDocumentCandidates,
        onInput: setInput,
        onReady: (value) => {
          api = value;
        },
        textareaRef,
      }),
    );
  }

  function moveCursorToEnd() {
    textarea.value = input;
    textarea.selectionStart = input.length;
    textarea.selectionEnd = input.length;
  }

  beforeEach(async () => {
    input = "";
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    textarea = document.createElement("textarea");
    textareaRef = { current: textarea };
    loadVaultFiles = async () => files;
    loadVaultTags = async () => [{ name: "project", files: [files[0]!] }];
    runtimeDocumentCandidates = [];
    await act(async () => {
      render();
    });
    await act(async () => {
      await Promise.resolve();
    });
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("builds mention candidates only while a mention query is active", async () => {
    await act(async () => {
      setInput("ask @Pol");
    });
    moveCursorToEnd();
    await act(async () => {
      api.syncMentionFromInput();
    });

    expect(api.mentionOpen).toBe(true);
    expect(api.mentionQuery).toBe("Pol");
    expect(
      api.mentionCandidates.some((item) => item.value === "Policies/"),
    ).toBe(true);

    await act(async () => {
      setInput("ask normally");
    });
    moveCursorToEnd();
    await act(async () => {
      api.syncMentionFromInput();
    });

    expect(api.mentionOpen).toBe(false);
    expect(api.mentionCandidates).toEqual([]);
  });

  it("selects a candidate as readable text with separate display metadata", async () => {
    await act(async () => {
      setInput("ask @");
    });
    moveCursorToEnd();
    await act(async () => {
      api.syncMentionFromInput();
    });

    const guide = api.mentionCandidates.find(
      (candidate) => candidate.value === "Policies/Guide.md",
    );
    expect(guide).toBeTruthy();

    await act(async () => {
      api.selectMention(guide!);
    });
    expect(input).toBe("ask Guide ");
    expect(input).not.toMatch(/@|\[|\]/);
    expect(api.displayMentions).toEqual([
      {
        kind: "file",
        value: "Policies/Guide.md",
        label: "Guide",
        range: { from: 4, to: 9 },
      },
    ]);
    expect(api.retrievalScope).toEqual({
      paths: [],
      pathPrefixes: [],
      requiredTags: [],
    });
  });

  it("shifts or safely unbinds annotations as the editable text changes", async () => {
    await act(async () => {
      setInput("ask @");
    });
    moveCursorToEnd();
    await act(async () => api.syncMentionFromInput());
    const guide = api.mentionCandidates.find(
      (candidate) => candidate.value === "Policies/Guide.md",
    )!;
    await act(async () => api.selectMention(guide));

    await act(async () =>
      api.handleInputChange(`please ${input}`, {
        from: 0,
        to: 0,
        insertedTextLength: 7,
      }),
    );
    expect(api.displayMentions[0]?.range).toEqual({ from: 11, to: 16 });

    await act(async () =>
      api.handleInputChange("please ask GuXide ", {
        from: 13,
        to: 13,
        insertedTextLength: 1,
      }),
    );
    expect(api.displayMentions).toEqual([]);
  });

  it("keeps the second repeated label bound when the same text is inserted at the start", async () => {
    await act(async () => setInput("Guide @"));
    moveCursorToEnd();
    await act(async () => api.syncMentionFromInput());
    const guide = api.mentionCandidates.find(
      (candidate) => candidate.value === "Policies/Guide.md",
    )!;
    await act(async () => api.selectMention(guide));
    expect(api.displayMentions[0]?.range).toEqual({ from: 6, to: 11 });

    await act(async () =>
      api.handleInputChange("Guide Guide Guide ", {
        from: 0,
        to: 0,
        insertedTextLength: 6,
      }),
    );

    expect(api.displayMentions[0]?.range).toEqual({ from: 12, to: 17 });
    expect(input.slice(12, 17)).toBe("Guide");
  });

  it("loads # tag candidates and maps them only to retrieval scope", async () => {
    await act(async () => setInput("ask #pro"));
    moveCursorToEnd();
    await act(async () => {
      api.syncMentionFromInput();
      await Promise.resolve();
    });

    const project = api.mentionCandidates.find(
      (candidate) => candidate.value === "project",
    );
    expect(project?.kind).toBe("tag");
    await act(async () => api.selectMention(project!));

    expect(input).toBe("ask project ");
    expect(api.retrievalScope).toEqual({
      paths: [],
      pathPrefixes: [],
      requiredTags: ["project"],
    });
  });

  it("closes the mention popover on Escape", async () => {
    await act(async () => {
      setInput("ask @Res");
    });
    moveCursorToEnd();
    await act(async () => {
      api.syncMentionFromInput();
    });
    expect(api.mentionOpen).toBe(true);

    const preventDefault = vi.fn();
    act(() => {
      api.handleComposerKeyDown({
        key: "Escape",
        preventDefault,
      } as unknown as React.KeyboardEvent<HTMLTextAreaElement>);
    });

    expect(preventDefault).toHaveBeenCalled();
    expect(api.mentionOpen).toBe(false);
  });

  it("refreshes stale vault files when opening mention suggestions", async () => {
    let currentFiles = files;
    loadVaultFiles = async () => currentFiles;
    await act(async () => {
      render();
      await Promise.resolve();
    });
    currentFiles = [
      ...files,
      {
        path: "Drafts/新建文档.md",
        title: "新建文档",
        updatedAt: "2026-01-02",
        isLocked: false,
      },
    ];

    await act(async () => {
      setInput("ask @新");
    });
    moveCursorToEnd();
    await act(async () => {
      api.syncMentionFromInput();
      await Promise.resolve();
    });

    expect(
      api.mentionCandidates.some((item) => item.value === "Drafts/新建文档.md"),
    ).toBe(true);
  });

  it("keeps @ file candidates available when tag metadata cannot be loaded", async () => {
    loadVaultTags = async () => Promise.reject(new Error("tag index offline"));
    await act(async () => {
      render();
      await Promise.resolve();
    });
    await act(async () => setInput("ask @Guid"));
    moveCursorToEnd();
    await act(async () => {
      api.syncMentionFromInput();
      await Promise.resolve();
    });

    expect(
      api.mentionCandidates.some(
        (candidate) => candidate.value === "Policies/Guide.md",
      ),
    ).toBe(true);
  });

  it("includes runtime document candidates that are not yet returned by fileList", async () => {
    runtimeDocumentCandidates = [
      {
        path: "Drafts/运行期文档.md",
        title: "运行期文档",
        updatedAt: "2026-01-02",
        isLocked: false,
      },
    ];
    await act(async () => {
      render();
    });

    await act(async () => {
      setInput("ask @运行");
    });
    moveCursorToEnd();
    await act(async () => {
      api.syncMentionFromInput();
      await Promise.resolve();
    });

    expect(
      api.mentionCandidates.some(
        (item) => item.value === "Drafts/运行期文档.md",
      ),
    ).toBe(true);
  });
});
