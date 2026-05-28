import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

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

const aiStreamDoc = (generated?: string) => ({
  type: "doc",
  content: [
    {
      type: "aiStream",
      attrs: {
        status: "ready",
        originalText: "原文内容",
        action: "rewrite",
      },
      content: generated ? [{ type: "text", text: generated }] : [],
    },
  ],
});

describe("buildInlineAiUserMessage", () => {
  it("uses action-specific prefix", () => {
    const msg = buildInlineAiUserMessage("rewrite", "hello");
    expect(msg).toContain("改写");
    expect(msg).toContain("hello");
  });
});

describe("AiStreamExtension accept / rollback / retry", () => {
  let editor: Editor;

  beforeEach(() => {
    editor = new Editor({
      extensions: [
        StarterKit.configure({ codeBlock: false }),
        AiStreamExtension,
      ],
      content: aiStreamDoc("生成结果"),
    });
  });

  afterEach(() => {
    editor.destroy();
  });

  it("accept keeps generated text as paragraph", () => {
    expect(editor.commands.acceptAiStream()).toBe(true);
    expect(editor.getText()).toBe("生成结果");
    expect(editor.state.doc.content.firstChild?.type.name).toBe("paragraph");
  });

  it("rollback restores originalText snapshot", () => {
    expect(editor.commands.rollbackAiStream()).toBe(true);
    expect(editor.getText()).toBe("原文内容");
  });

  it("retry callback is separate from rollback", () => {
    const onRetry = vi.fn();
    editor.destroy();
    editor = new Editor({
      extensions: [
        StarterKit.configure({ codeBlock: false }),
        AiStreamExtension.configure({ onRetry }),
      ],
      content: aiStreamDoc("生成结果"),
    });

    const ext = editor.extensionManager.extensions.find(
      (e) => e.name === "aiStream",
    );
    const retry = (ext?.options as { onRetry?: (ed: Editor) => void }).onRetry;
    retry?.(editor);

    expect(onRetry).toHaveBeenCalledOnce();
    expect(editor.getText()).toBe("生成结果");

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
      extensions: [
        StarterKit.configure({ codeBlock: false }),
        AiStreamExtension,
      ],
      content: aiStreamDoc(),
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
});
