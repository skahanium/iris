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
const assistantExecute = vi.fn();

vi.mock("@/lib/ipc", () => ({
  llmGenerate: (...args: unknown[]) => llmGenerate(...args),
  llmAbort: (...args: unknown[]) => llmAbort(...args),
  assistantExecute: (...args: unknown[]) => assistantExecute(...args),
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
    assistantExecute.mockReset();
    llmGenerate.mockResolvedValue("req-1");
    llmAbort.mockResolvedValue(undefined);
    assistantExecute.mockResolvedValue({
      kind: "writing",
      payload: {
        request_id: "classified-req-1",
        suggestions: [],
        patches: [
          {
            id: "patch-classified",
            target_path: ".classified/secret.md",
            base_content_hash: "hash",
            range: { start: 0, end: 4 },
            original_text: "原文内容",
            replacement_text: "涉密改写结果",
            evidence_packet_ids: [],
            risk_level: "low",
            warnings: [],
            created_at: "2026-06-27T00:00:00",
          },
        ],
        evidence_used: [],
        total_tokens: {
          prompt_tokens: 0,
          completion_tokens: 0,
          total_tokens: 0,
        },
        writing_state: null,
      },
      requestId: "classified-req-1",
      runStatus: "completed",
      artifacts: [],
    });
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

  it("classified inline rewrite uses assistantExecute instead of normal llmGenerate", async () => {
    function ClassifiedHost() {
      api = useInlineAi({
        provider: "openai",
        domain: "classified",
        notePath: ".classified/secret.md",
        getNoteContent: () => "# Secret\n\n原文内容",
      });
      return null;
    }
    act(() => {
      root.render(createElement(ClassifiedHost));
    });
    const editor = new Editor({
      extensions: editorExtensions,
      content: {
        type: "doc",
        content: [
          {
            type: "paragraph",
            content: [{ type: "text", text: "原文内容" }],
          },
        ],
      },
    });
    editor.commands.setTextSelection({ from: 1, to: 5 });

    await act(async () => {
      await api.run(editor, "rewrite");
    });

    expect(llmGenerate).not.toHaveBeenCalled();
    expect(assistantExecute).toHaveBeenCalledTimes(1);
    expect(assistantExecute).toHaveBeenCalledWith(
      expect.objectContaining({
        aiDomain: "classified",
        agentIntent: "rewrite_selection",
        intent: "writing",
        notePath: ".classified/secret.md",
        selection: "原文内容",
        noteContent: null,
        contextReferences: [
          expect.objectContaining({
            kind: "selection",
            filePath: ".classified/secret.md",
            editorRange: { from: 1, to: 5 },
            excerpt: "原文内容",
            stale: false,
          }),
        ],
      }),
    );
    const classifiedRequest = assistantExecute.mock.calls[0]?.[0];
    expect(classifiedRequest.contextReferences[0].contentHash).toMatch(
      /^[0-9a-f]{8}$/,
    );
    expect(editor.getText()).toContain("涉密改写结果");

    editor.destroy();
  });

  it("keeps long selection body out of the assistant user message", async () => {
    function ClassifiedHost() {
      api = useInlineAi({
        provider: "openai",
        domain: "classified",
        notePath: ".classified/secret.md",
        getNoteContent: () => "# Secret\n\n长选区正文不应进入 userMessage",
      });
      return null;
    }
    act(() => {
      root.render(createElement(ClassifiedHost));
    });
    const sentinel = "长选区正文不应进入 userMessage。".repeat(6);
    const editor = new Editor({
      extensions: editorExtensions,
      content: {
        type: "doc",
        content: [
          {
            type: "paragraph",
            content: [{ type: "text", text: sentinel }],
          },
        ],
      },
    });
    editor.commands.setTextSelection({ from: 1, to: 1 + sentinel.length });

    await act(async () => {
      await api.run(editor, "rewrite");
    });

    expect(assistantExecute).toHaveBeenCalledTimes(1);
    const request = assistantExecute.mock.calls[0]?.[0];
    expect(request.agentIntent).toBe("rewrite_selection");
    expect(request.intent).toBe("writing");
    // The full long selection must never be pasted into the prompt-facing
    // user message — it flows via the bounded `selection` and
    // `contextReferences.excerpt` channels instead.
    expect(request.message).not.toContain("长选区正文不应进入 userMessage");
    expect(request.selection).toBe(sentinel);
    expect(request.contextReferences[0]).toMatchObject({
      kind: "selection",
      filePath: ".classified/secret.md",
    });
    // The excerpt carried by the reference is bounded, so even a long
    // selection cannot leak its full body through the context reference.
    expect((request.contextReferences[0]?.excerpt ?? "").length).toBeLessThan(
      sentinel.length,
    );

    editor.destroy();
  });

  it("classified slash command uses classified chat without leaking note markdown as normal llm system", async () => {
    assistantExecute.mockResolvedValueOnce({
      kind: "chat",
      payload: {
        request_id: "classified-chat-1",
        session_id: 0,
        status: "completed",
        content: "涉密插入结果",
        tool_calls: [],
        tool_results: [],
        harness_rounds: 1,
        usage: { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 },
        usage_source: "provider",
        citation_valid: true,
        evidence_packets: [],
        pending_confirmation: false,
      },
      requestId: "classified-chat-1",
      runStatus: "completed",
      artifacts: [],
    });
    function ClassifiedHost() {
      api = useInlineAi({
        provider: "openai",
        domain: "classified",
        notePath: ".classified/secret.md",
        getNoteContent: () => "# Secret\n\n不可外泄正文",
      });
      return null;
    }
    act(() => {
      root.render(createElement(ClassifiedHost));
    });
    const editor = new Editor({
      extensions: editorExtensions,
      content: { type: "doc", content: [{ type: "paragraph" }] },
    });

    await act(async () => {
      await api.runSlash(editor, "续写", "# Secret\n\n不可外泄正文");
    });

    expect(llmGenerate).not.toHaveBeenCalled();
    expect(assistantExecute).toHaveBeenCalledWith(
      expect.objectContaining({
        aiDomain: "classified",
        intent: "chat",
        notePath: ".classified/secret.md",
        noteContent: null,
        contextReferences: [],
      }),
    );
    expect(editor.getText()).toContain("涉密插入结果");

    editor.destroy();
  });
});
