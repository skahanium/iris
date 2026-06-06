import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AiSourceHighlightExtension } from "@/components/editor/extensions/AiSourceHighlightExtension";
import { AiStreamExtension } from "@/components/editor/extensions/AiStreamExtension";
import { getActiveAiStreamAttrs, useInlineAi } from "@/hooks/useInlineAi";
import { buildInlineAiUserMessage } from "@/lib/inline-ai-prompts";

const llmGenerate = vi.fn();
const llmAbort = vi.fn();

vi.mock("@/lib/ipc", () => ({
  llmGenerate: (...args: unknown[]) => llmGenerate(...args),
  llmAbort: (...args: unknown[]) => llmAbort(...args),
  listenLlmToken: vi.fn().mockResolvedValue(() => {}),
  listenLlmDone: vi.fn().mockResolvedValue(() => {}),
  listenLlmError: vi.fn().mockResolvedValue(() => {}),
}));

/** 原文段落 + 下方 aiStream（对照模式） */
function inlineCandidateDoc(generated?: string) {
  return {
    type: "doc",
    content: [
      {
        type: "paragraph",
        content: [{ type: "text", text: "原文内容" }],
      },
      {
        type: "aiStream",
        attrs: {
          status: "ready",
          originalText: "原文内容",
          action: "rewrite",
          sourceFrom: 1,
          sourceTo: 5,
        },
        content: generated ? [{ type: "text", text: generated }] : [],
      },
    ],
  };
}

const editorExtensions = [
  StarterKit.configure({ codeBlock: false }),
  AiSourceHighlightExtension,
  AiStreamExtension,
];

describe("buildInlineAiUserMessage", () => {
  it("uses action-specific prefix", () => {
    const msg = buildInlineAiUserMessage("rewrite", "hello");
    expect(msg).toContain("改写");
    expect(msg).toContain("hello");
  });
});

describe("AiStreamExtension insert below selection", () => {
  let editor: Editor;

  beforeEach(() => {
    editor = new Editor({
      extensions: editorExtensions,
      content: {
        type: "doc",
        content: [
          {
            type: "paragraph",
            content: [{ type: "text", text: "前文 选区文字 后文" }],
          },
        ],
      },
    });
  });

  afterEach(() => {
    editor.destroy();
  });

  it("keeps original text and inserts aiStream after block", () => {
    editor.commands.setTextSelection({ from: 4, to: 8 });
    expect(
      editor.commands.insertAiStreamBelowSelection({
        originalText: "选区文字",
        action: "translate",
        sourceFrom: 4,
        sourceTo: 8,
      }),
    ).toBe(true);

    expect(editor.getText()).toContain("选区文字");
    expect(editor.getText()).toContain("前文");
    expect(editor.state.doc.content.childCount).toBe(2);
    expect(editor.state.doc.content.lastChild?.type.name).toBe("aiStream");

    const mark = editor.state.schema.marks.aiSourceHighlight;
    let highlighted = false;
    editor.state.doc.descendants((node) => {
      if (
        node.isText &&
        node.marks.some((m) => m.type === mark) &&
        node.text === "选区文字"
      ) {
        highlighted = true;
      }
    });
    expect(highlighted).toBe(true);
  });
});

describe("AiStreamExtension accept / rollback / dismiss", () => {
  let editor: Editor;

  beforeEach(() => {
    editor = new Editor({
      extensions: editorExtensions,
      content: inlineCandidateDoc("生成结果"),
    });
  });

  afterEach(() => {
    editor.destroy();
  });

  it("accept replaces source range with generated text and removes aiStream", async () => {
    expect(editor.commands.acceptAiStream()).toBe(true);
    await Promise.resolve();
    expect(editor.getText()).toBe("生成结果");
    expect(editor.state.doc.content.childCount).toBe(1);
    expect(editor.state.doc.content.firstChild?.type.name).toBe("paragraph");
  });

  it("undo after accept restores original text without aiStream panel", async () => {
    expect(editor.commands.acceptAiStream()).toBe(true);
    await Promise.resolve();
    expect(editor.getText()).toBe("生成结果");

    expect(editor.commands.undo()).toBe(true);
    expect(editor.getText()).toBe("原文内容");

    let hasAiStream = false;
    editor.state.doc.descendants((node) => {
      if (node.type.name === "aiStream") hasAiStream = true;
    });
    expect(hasAiStream).toBe(false);
  });

  it("rollback removes aiStream only and keeps original paragraph", () => {
    expect(editor.commands.rollbackAiStream()).toBe(true);
    expect(editor.getText()).toBe("原文内容");
    expect(editor.state.doc.content.childCount).toBe(1);
  });

  it("dismiss invokes onDismiss then rollback", () => {
    const onDismiss = vi.fn();
    editor.destroy();
    editor = new Editor({
      extensions: [
        ...editorExtensions.slice(0, -1),
        AiStreamExtension.configure({ onDismiss }),
      ],
      content: inlineCandidateDoc("生成结果"),
    });

    expect(editor.commands.dismissAiStream()).toBe(true);
    expect(onDismiss).toHaveBeenCalledOnce();
    expect(editor.getText()).toBe("原文内容");
  });

  it("retry callback is separate from rollback", () => {
    const onRetry = vi.fn();
    editor.destroy();
    editor = new Editor({
      extensions: [
        ...editorExtensions.slice(0, -1),
        AiStreamExtension.configure({ onRetry }),
      ],
      content: inlineCandidateDoc("生成结果"),
    });

    const ext = editor.extensionManager.extensions.find(
      (e) => e.name === "aiStream",
    );
    const retry = (ext?.options as { onRetry?: (ed: Editor) => void }).onRetry;
    retry?.(editor);

    expect(onRetry).toHaveBeenCalledOnce();
    expect(editor.getText()).toContain("原文内容");
    expect(editor.getText()).toContain("生成结果");

    expect(editor.commands.rollbackAiStream()).toBe(true);
    expect(editor.getText()).toBe("原文内容");
    expect(onRetry).toHaveBeenCalledOnce();
  });

  it("getActiveAiStreamAttrs reads node metadata for retry", () => {
    const ctx = getActiveAiStreamAttrs(editor);
    expect(ctx).toEqual({ originalText: "原文内容", action: "rewrite" });
  });
});

describe("useInlineAi with mocked IPC", () => {
  let root: Root;
  let container: HTMLDivElement;
  let api: ReturnType<typeof useInlineAi>;

  function Host() {
    api = useInlineAi({ provider: "openai" });
    return null;
  }

  beforeEach(() => {
    llmGenerate.mockReset();
    llmAbort.mockReset();
    llmGenerate.mockResolvedValue("req-1");
    llmAbort.mockResolvedValue(undefined);
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    act(() => {
      root.render(createElement(Host));
    });
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("retry calls llmGenerate with snapshot from ai-stream attrs", async () => {
    const editor = new Editor({
      extensions: editorExtensions,
      content: inlineCandidateDoc(),
    });

    await act(async () => {
      await api.retry(editor);
    });

    expect(llmGenerate).toHaveBeenCalledTimes(1);
    const prompt = llmGenerate.mock.calls[0]?.[0]?.messages[0]?.content;
    expect(prompt).toContain("改写");
    expect(prompt).toContain("原文内容");

    llmGenerate.mockResolvedValueOnce("req-2");
    await act(async () => {
      await api.retry(editor);
    });

    expect(llmGenerate).toHaveBeenCalledTimes(2);
    expect(llmGenerate.mock.calls[1]?.[0]?.messages[0]?.content).toBe(prompt);

    editor.destroy();
  });

  it("sets aiStream status to ready when llmGenerate resolves", async () => {
    const editor = new Editor({
      extensions: editorExtensions,
      content: inlineCandidateDoc(),
    });

    await act(async () => {
      await api.retry(editor);
    });

    let status = "";
    editor.state.doc.descendants((node) => {
      if (node.type.name === "aiStream") {
        status = node.attrs.status as string;
      }
    });
    expect(status).toBe("ready");

    editor.destroy();
  });

  it("dismiss aborts in-flight request", async () => {
    const editor = new Editor({
      extensions: editorExtensions,
      content: inlineCandidateDoc(),
    });

    await act(async () => {
      await api.retry(editor);
    });

    expect(llmGenerate).toHaveBeenCalledTimes(1);

    await act(async () => {
      api.dismiss(editor);
    });

    expect(llmAbort).toHaveBeenCalledWith("req-1");

    editor.destroy();
  });
});
