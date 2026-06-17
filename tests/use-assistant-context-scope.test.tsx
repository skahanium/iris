import { act, createElement, type RefObject } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useAssistantContextScope } from "@/components/ai/hooks/useAssistantContextScope";
import type { FileListItem } from "@/types/ipc";

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
  onInput,
  onReady,
  textareaRef,
}: {
  input: string;
  onInput: (next: string | ((prev: string) => string)) => void;
  onReady: (api: HookApi) => void;
  textareaRef: RefObject<HTMLTextAreaElement | null>;
}) {
  const api = useAssistantContextScope({
    input,
    setInput: onInput,
    textareaRef,
    loadVaultFiles: async () => files,
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

  function setInput(next: string | ((prev: string) => string)) {
    input = typeof next === "function" ? next(input) : next;
    render();
  }

  function render() {
    root.render(
      createElement(Harness, {
        input,
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

  it("selects and removes mention tokens through a stable hook API", async () => {
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
    expect(input).toContain("@[Policies/Guide.md]");

    const [token] = api.mentionTokens;
    await act(async () => {
      api.removeMentionToken(token!);
    });
    expect(input).not.toContain("@[");
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
});
