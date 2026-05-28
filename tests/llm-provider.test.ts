import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AiStreamExtension } from "@/components/editor/extensions/AiStreamExtension";
import { useInlineAi } from "@/hooks/useInlineAi";

const llmGenerate = vi.fn();
const llmAbort = vi.fn();

vi.mock("@/lib/ipc", () => ({
  llmGenerate: (...args: unknown[]) => llmGenerate(...args),
  llmAbort: (...args: unknown[]) => llmAbort(...args),
  listenLlmToken: vi.fn().mockResolvedValue(() => {}),
  listenLlmDone: vi.fn().mockResolvedValue(() => {}),
  listenLlmError: vi.fn().mockResolvedValue(() => {}),
}));

describe("shared LLM provider for inline and slash", () => {
  let root: Root;
  let container: HTMLDivElement;
  let api: ReturnType<typeof useInlineAi>;
  let provider = "openai";

  function Host() {
    api = useInlineAi({ provider, onStatus: () => {} });
    return null;
  }

  beforeEach(() => {
    llmGenerate.mockReset();
    llmAbort.mockReset();
    llmGenerate.mockResolvedValue("req-1");
    llmAbort.mockResolvedValue(undefined);
    provider = "openai";
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

  it("runSlash uses the configured provider", async () => {
    const editor = new Editor({
      extensions: [
        StarterKit.configure({ codeBlock: false }),
        AiStreamExtension,
      ],
      content: "<p></p>",
    });

    await act(async () => {
      await api.runSlash(editor, "summarize", "# Note\n\nBody");
    });

    expect(llmGenerate).toHaveBeenCalledWith(
      expect.objectContaining({ provider: "openai", stream: true }),
    );
    expect(llmGenerate.mock.calls[0]?.[0]?.system).toContain("Note");

    provider = "ollama";
    act(() => {
      root.render(createElement(Host));
    });

    llmGenerate.mockResolvedValueOnce("req-2");
    await act(async () => {
      await api.runSlash(editor, "outline", "content");
    });

    expect(llmGenerate).toHaveBeenLastCalledWith(
      expect.objectContaining({ provider: "ollama" }),
    );

    editor.destroy();
  });

  it("run uses provider from hook options", async () => {
    provider = "custom";
    act(() => {
      root.render(createElement(Host));
    });

    const editor = new Editor({
      extensions: [
        StarterKit.configure({ codeBlock: false }),
        AiStreamExtension,
      ],
      content: "<p>hello</p>",
    });
    editor.commands.setTextSelection({ from: 1, to: 6 });

    await act(async () => {
      await api.run(editor, "rewrite");
    });

    expect(llmGenerate).toHaveBeenCalledWith(
      expect.objectContaining({ provider: "custom" }),
    );

    editor.destroy();
  });
});
