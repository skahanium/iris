import { Editor } from "@tiptap/core";
import { afterEach, describe, expect, it } from "vitest";

import { AssistantMentionExtension } from "@/components/ai/extensions/AssistantMentionExtension";
import { createAssistantComposerExtensions } from "@/components/ai/extensions/assistant-composer-extensions";
import { projectAssistantComposerDoc } from "@/lib/assistant-composer-doc";

describe("assistant composer document projection", () => {
  let editor: Editor | null = null;

  afterEach(() => {
    editor?.destroy();
    editor = null;
  });

  it("serializes atom mentions to readable text and exact UTF-16 ranges", () => {
    editor = new Editor({
      extensions: createAssistantComposerExtensions({
        mentionExtension: AssistantMentionExtension,
      }),
    });
    editor.commands.setContent({
      type: "doc",
      content: [
        {
          type: "paragraph",
          content: [
            { type: "text", text: "😀 查 " },
            {
              type: "assistantMention",
              attrs: {
                kind: "file",
                value: "Research/Guide.md",
                label: "Guide",
              },
            },
            { type: "text", text: "\n继续" },
          ],
        },
      ],
    });

    expect(projectAssistantComposerDoc(editor.state.doc)).toEqual({
      text: "😀 查 Guide\n继续",
      displayMentions: [
        {
          kind: "file",
          value: "Research/Guide.md",
          label: "Guide",
          range: { from: 5, to: 10 },
        },
      ],
    });
  });

  it("keeps mention nodes atomic while surrounding whitespace remains editable", () => {
    editor = new Editor({
      extensions: createAssistantComposerExtensions({
        mentionExtension: AssistantMentionExtension,
      }),
    });
    editor.commands.setContent({
      type: "doc",
      content: [
        {
          type: "paragraph",
          content: [
            { type: "text", text: "前 " },
            {
              type: "assistantMention",
              attrs: {
                kind: "folder",
                value: "Research/",
                label: "Research",
              },
            },
            { type: "text", text: " 后" },
          ],
        },
      ],
    });

    const mention = editor.state.doc.firstChild?.child(1);
    expect(mention?.type.name).toBe("assistantMention");
    expect(mention?.isAtom).toBe(true);
    expect(mention?.isLeaf).toBe(true);
  });
});
