import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { describe, expect, it } from "vitest";

import { WikiLinkExtension } from "@/components/editor/extensions/WikiLinkExtension";
import type { WikiLinkSuggestionItem } from "@/lib/wiki-link-suggestions";

const suggestions: WikiLinkSuggestionItem[] = [
  {
    id: "党纪国法/中国共产党组织处理规定（试行）.md",
    title: "中国共产党组织处理规定（试行）",
    path: "党纪国法/中国共产党组织处理规定（试行）.md",
    keywords: "中国共产党组织处理规定（试行） 党纪国法",
  },
];

function createEditor(): Editor {
  const element = document.createElement("div");
  document.body.appendChild(element);
  return new Editor({
    element,
    extensions: [
      StarterKit,
      WikiLinkExtension.configure({
        getSuggestions: async () => suggestions,
      }),
    ],
    content: "<p></p>",
  });
}

async function flushSuggestionRenderer() {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 0));
}

function wikiSuggestionStates(editor: Editor) {
  return editor.state.plugins
    .filter((plugin) =>
      ((plugin as { key?: string }).key ?? "").startsWith("wikiLinkSuggestion"),
    )
    .map((plugin) => plugin.getState(editor.state));
}

describe("WikiLinkExtension suggestion triggers", () => {
  it("opens suggestions after typing the ASCII trigger", async () => {
    const editor = createEditor();
    try {
      editor.commands.insertContent("[[");
      await flushSuggestionRenderer();

      expect(wikiSuggestionStates(editor).some((state) => state.active)).toBe(
        true,
      );
      expect(document.querySelector(".tippy-box")).not.toBeNull();
    } finally {
      editor.destroy();
      document.body.innerHTML = "";
    }
  });

  it("normalizes and opens suggestions after typing the full-width trigger", async () => {
    const editor = createEditor();
    try {
      editor.commands.insertContent("【【");
      await flushSuggestionRenderer();

      expect(editor.state.doc.textContent).toBe("[[");
      expect(wikiSuggestionStates(editor).some((state) => state.active)).toBe(
        true,
      );
      expect(document.querySelector(".tippy-box")).not.toBeNull();
    } finally {
      editor.destroy();
      document.body.innerHTML = "";
    }
  });
});
