import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { describe, expect, it, vi } from "vitest";

import { runEditorAction } from "@/lib/editor-action-executor";

describe("runEditorAction", () => {
  it("pastes clipboard text for clipboard kind", async () => {
    const readText = vi.fn().mockResolvedValue("粘贴内容");
    Object.assign(navigator, {
      clipboard: { readText },
    });

    const editor = new Editor({
      extensions: [StarterKit],
      content: "<p>原文</p>",
    });
    editor.commands.selectAll();

    await runEditorAction("paste", editor, {
      onInlineAi: vi.fn(),
      onSlashCommand: vi.fn(),
      onSendToAi: vi.fn(),
    });

    expect(readText).toHaveBeenCalled();
    expect(editor.getText()).toContain("粘贴内容");

    editor.destroy();
  });

  it("routes summarize to slash command handler", async () => {
    const onSlashCommand = vi.fn();
    const editor = new Editor({
      extensions: [StarterKit],
      content: "<p>x</p>",
    });

    await runEditorAction("summarize", editor, {
      onInlineAi: vi.fn(),
      onSlashCommand,
      onSendToAi: vi.fn(),
    });

    expect(onSlashCommand).toHaveBeenCalledWith("summarize");
    editor.destroy();
  });
});
