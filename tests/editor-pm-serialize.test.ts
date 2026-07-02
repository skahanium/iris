import { MarkdownSerializer } from "prosemirror-markdown";
import { afterEach, describe, expect, it, vi } from "vitest";

import * as markdownLib from "@/lib/markdown";

import {
  createProductionEditorFromBody,
  createProductionEditorFromIngestedBody,
  fullNoteRoundTrip,
  normalizeMd,
  pmSerializeBody,
} from "./helpers/tiptap-serialize-harness";

function typeTextThroughInputRules(
  editor: ReturnType<typeof createProductionEditorFromIngestedBody>,
  text: string,
): void {
  for (const ch of text) {
    const { from, to } = editor.state.selection;
    let handled = false;
    editor.view.someProp("handleTextInput", (handler) => {
      if (handler(editor.view, from, to, ch, () => editor.state.tr)) {
        handled = true;
        return true;
      }
      return false;
    });
    if (!handled) {
      editor.commands.insertContent(ch);
    }
  }
}

function pressEnter(
  editor: ReturnType<typeof createProductionEditorFromIngestedBody>,
): void {
  editor.view.focus();
  editor.view.dom.dispatchEvent(
    new KeyboardEvent("keydown", {
      key: "Enter",
      code: "Enter",
      keyCode: 13,
      bubbles: true,
      cancelable: true,
    }),
  );
}

describe("editorDocToMarkdown (prosemirror-markdown hot path)", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("serializes a simple paragraph without Turndown fallback", () => {
    const turndownSpy = vi
      .spyOn(markdownLib, "editorBodyHtmlToMarkdown")
      .mockImplementation(() => {
        throw new Error("Turndown must not run for native GFM");
      });

    const editor = createProductionEditorFromBody("Hello **world**.");
    try {
      const md = pmSerializeBody(editor);
      expect(md).toContain("Hello");
      expect(md).toContain("world");
      expect(turndownSpy).not.toHaveBeenCalled();
    } finally {
      editor.destroy();
    }
  });

  it("round-trips wiki-links via PM serializer", () => {
    const turndownSpy = vi.spyOn(markdownLib, "editorBodyHtmlToMarkdown");

    const body = "See [[Architecture Notes]] for details.";
    const editor = createProductionEditorFromBody(body);
    try {
      const md = pmSerializeBody(editor);
      expect(md).toContain("[[Architecture Notes]]");
      expect(turndownSpy).not.toHaveBeenCalled();
    } finally {
      editor.destroy();
    }
  });

  it("round-trips GFM tables and task lists via PM serializer", () => {
    const turndownSpy = vi.spyOn(markdownLib, "editorBodyHtmlToMarkdown");

    const body = [
      "- [x] Done",
      "- [ ] Todo",
      "",
      "| A | B |",
      "| --- | --- |",
      "| 1 | 2 |",
    ].join("\n");

    const editor = createProductionEditorFromBody(body);
    try {
      const md = pmSerializeBody(editor);
      expect(md).toContain("- [x] Done");
      expect(md).toContain("- [ ] Todo");
      expect(md).toContain("| A | B |");
      expect(md).toContain("| 1 | 2 |");
      expect(turndownSpy).not.toHaveBeenCalled();
    } finally {
      editor.destroy();
    }
  });

  it("serializes ordinary nested lists without Turndown fallback or stderr", () => {
    const turndownSpy = vi.spyOn(markdownLib, "editorBodyHtmlToMarkdown");
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    const editor = createProductionEditorFromIngestedBody(
      ["- one", "  - two", "- three", "", "1. first", "   1. second"].join(
        "\n",
      ),
    );
    try {
      const md = pmSerializeBody(editor);
      expect(md).toContain("- one");
      expect(md).toContain("two");
      expect(md).toContain("1. first");
      expect(md).toContain("second");
      expect(turndownSpy).not.toHaveBeenCalled();
      expect(errorSpy).not.toHaveBeenCalled();
    } finally {
      editor.destroy();
    }
  });

  it("serializes unresolved AI stream nodes without persisting suggestions or stderr", () => {
    const turndownSpy = vi.spyOn(markdownLib, "editorBodyHtmlToMarkdown");
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    const editor = createProductionEditorFromBody("Original paragraph.");
    try {
      editor.commands.insertAiStreamAtCursor({
        originalText: "Original paragraph.",
        action: "rewrite",
      });
      editor.commands.updateAiStream("Temporary rewrite");

      const md = pmSerializeBody(editor);
      expect(md).toContain("Original paragraph.");
      expect(md).not.toContain("Temporary rewrite");
      expect(turndownSpy).not.toHaveBeenCalled();
      expect(errorSpy).not.toHaveBeenCalled();
    } finally {
      editor.destroy();
    }
  });

  it("round-trips images via PM serializer", () => {
    const turndownSpy = vi.spyOn(markdownLib, "editorBodyHtmlToMarkdown");

    const body = "![diagram](assets/example.png)";
    const editor = createProductionEditorFromIngestedBody(body);
    try {
      const md = pmSerializeBody(editor);
      expect(md).toContain("![diagram](assets/example.png)");
      expect(turndownSpy).not.toHaveBeenCalled();
    } finally {
      editor.destroy();
    }
  });

  it("round-trips Obsidian-style media embeds without converting them to markdown images", () => {
    const turndownSpy = vi.spyOn(markdownLib, "editorBodyHtmlToMarkdown");

    const body = ["![[diagram.png]]", "", "![[paper.pdf|证据材料]]"].join("\n");
    const editor = createProductionEditorFromIngestedBody(body);
    try {
      const md = normalizeMd(pmSerializeBody(editor));
      expect(md).toContain("![[diagram.png]]");
      expect(md).toContain("![[paper.pdf|证据材料]]");
      expect(md).not.toContain("![diagram.png](diagram.png)");
      expect(turndownSpy).not.toHaveBeenCalled();
    } finally {
      editor.destroy();
    }
  });

  it("renders vault-relative editor images through Tauri asset URLs without changing markdown src", () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {
        convertFileSrc: (filePath: string, protocol = "asset") =>
          `${protocol}://localhost/${filePath}`,
      },
    });

    const editor = createProductionEditorFromIngestedBody(
      "![diagram](assets/example.png)",
      "/Users/example/Vault",
    );
    try {
      expect(editor.view.dom.innerHTML).toContain(
        'src="asset://localhost//Users/example/Vault/assets/example.png"',
      );
      expect(editor.getHTML()).toContain('src="assets/example.png"');
      expect(pmSerializeBody(editor)).toContain(
        "![diagram](assets/example.png)",
      );
    } finally {
      Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
      editor.destroy();
    }
  });

  it("round-trips hard line breaks via PM serializer", () => {
    const turndownSpy = vi.spyOn(markdownLib, "editorBodyHtmlToMarkdown");

    const body = "Line one  \nLine two";
    const editor = createProductionEditorFromIngestedBody(body);
    try {
      const md = pmSerializeBody(editor);
      expect(md).toMatch(/Line one\\\nLine two/);
      expect(turndownSpy).not.toHaveBeenCalled();
    } finally {
      editor.destroy();
    }
  });

  it("round-trips preserve-only callout blocks via PM serializer", () => {
    const turndownSpy = vi.spyOn(markdownLib, "editorBodyHtmlToMarkdown");

    const body = "> [!note] Info\n> Callout body.";
    const editor = createProductionEditorFromIngestedBody(body);
    try {
      const md = pmSerializeBody(editor);
      expect(normalizeMd(md)).toBe(normalizeMd(body));
      expect(turndownSpy).not.toHaveBeenCalled();
    } finally {
      editor.destroy();
    }
  });

  it("does not amplify blank lines across headings, tasks, tables, and callouts", () => {
    const body = [
      "# Heading",
      "",
      "Paragraph A.",
      "",
      "- [x] Done",
      "- [ ] Todo",
      "",
      "| A | B |",
      "| --- | --- |",
      "| 1 | 2 |",
      "",
      "> [!note] Info",
      "> Body.",
      "",
      "Paragraph B.",
    ].join("\n");
    const editor = createProductionEditorFromIngestedBody(body);
    try {
      const md = pmSerializeBody(editor).replace(/\r\n/g, "\n");
      expect(md).toContain("# Heading");
      expect(md).toContain("- [x] Done");
      expect(md).toContain("| A | B |");
      expect(md).toContain("> [!note] Info");
      expect(md).not.toMatch(/\n{3,}/);
    } finally {
      editor.destroy();
    }
  });

  it("preserves ordinary spaces in headings and paragraphs through ingest and save", () => {
    const body = ["## 第一章    总 则", "", "正文    保留  空格"].join("\n");
    const editor = createProductionEditorFromIngestedBody(body);
    try {
      const md = pmSerializeBody(editor).replace(/\r\n/g, "\n");

      expect(md).toContain("## 第一章    总 则");
      expect(md).toContain("正文    保留  空格");
    } finally {
      editor.destroy();
    }
  });

  it("preserves ordinary spaces after reopening serialized markdown", () => {
    const body = ["## 第一章    总 则", "", "正文    保留  空格"].join("\n");
    const first = createProductionEditorFromIngestedBody(body);
    try {
      const saved = pmSerializeBody(first).replace(/\r\n/g, "\n");
      const reopened = createProductionEditorFromIngestedBody(saved);
      try {
        const reopenedMd = pmSerializeBody(reopened).replace(/\r\n/g, "\n");

        expect(reopenedMd).toContain("## 第一章    总 则");
        expect(reopenedMd).toContain("正文    保留  空格");
      } finally {
        reopened.destroy();
      }
    } finally {
      first.destroy();
    }
  });

  it("does not reintroduce a trailing blank line inside fenced code blocks", () => {
    const body = [
      "```bash",
      "# 一行安装，或通过 npm 安装",
      "curl -fsSL https://mimo.xiaomi.com/install | bash",
      "npm install -g @mimo-ai/cli",
      "```",
    ].join("\n");
    const editor = createProductionEditorFromIngestedBody(body);
    try {
      const md = pmSerializeBody(editor).replace(/\r\n/g, "\n");

      expect(md).toContain("npm install -g @mimo-ai/cli\n```");
      expect(md).not.toContain("npm install -g @mimo-ai/cli\n\n```");

      const reopened = createProductionEditorFromIngestedBody(md);
      try {
        const reopenedMd = pmSerializeBody(reopened).replace(/\r\n/g, "\n");
        expect(reopenedMd).toContain("npm install -g @mimo-ai/cli\n```");
        expect(reopenedMd).not.toContain("npm install -g @mimo-ai/cli\n\n```");
      } finally {
        reopened.destroy();
      }
    } finally {
      editor.destroy();
    }
  });

  it("keeps fenced code block content free of trailing newline in the production DOM", () => {
    const body = [
      "```bash",
      "# 一行安装，或通过 npm 安装",
      "curl -fsSL https://mimo.xiaomi.com/install | bash",
      "npm install -g @mimo-ai/cli",
      "```",
    ].join("\n");
    const editor = createProductionEditorFromIngestedBody(body);
    try {
      const code = editor.view.dom.querySelector("pre code");
      expect(code).toBeInstanceOf(HTMLElement);
      expect(code?.textContent).toBe(
        [
          "# 一行安装，或通过 npm 安装",
          "curl -fsSL https://mimo.xiaomi.com/install | bash",
          "npm install -g @mimo-ai/cli",
        ].join("\n"),
      );
      expect(code?.textContent).not.toMatch(/\n$/);
    } finally {
      editor.destroy();
    }
  });

  it("round-trips Iris indented paragraph HTML as an editable block", () => {
    const editor = createProductionEditorFromIngestedBody(
      '<p data-iris-indent="2"><strong>Bold</strong> text</p>',
    );
    try {
      let paragraphAttrs: Record<string, unknown> | null = null;
      editor.state.doc.descendants((node) => {
        if (node.type.name === "paragraph") {
          paragraphAttrs = node.attrs as Record<string, unknown>;
          return false;
        }
      });

      expect(paragraphAttrs).toMatchObject({ irisIndent: 2 });
      expect(editor.getText()).toBe("Bold text");
      expect(pmSerializeBody(editor)).toContain(
        '<p data-iris-indent="2"><strong>Bold</strong> text</p>',
      );
    } finally {
      editor.destroy();
    }
  });

  it("round-trips Iris indented heading HTML as an editable block", () => {
    const editor = createProductionEditorFromIngestedBody(
      '<h2 data-iris-indent="1">Heading</h2>',
    );
    try {
      let headingAttrs: Record<string, unknown> | null = null;
      editor.state.doc.descendants((node) => {
        if (node.type.name === "heading") {
          headingAttrs = node.attrs as Record<string, unknown>;
          return false;
        }
      });

      expect(headingAttrs).toMatchObject({ irisIndent: 1, level: 2 });
      expect(pmSerializeBody(editor)).toContain(
        '<h2 data-iris-indent="1">Heading</h2>',
      );
    } finally {
      editor.destroy();
    }
  });

  it("keeps ordinary raw HTML preserve-only instead of treating it as Iris block indent", () => {
    const editor = createProductionEditorFromIngestedBody(
      '<div class="raw">Raw HTML</div>',
    );
    try {
      let preserveBlockCount = 0;
      editor.state.doc.descendants((node) => {
        if (node.type.name === "preserveBlock") {
          preserveBlockCount += 1;
        }
      });

      expect(preserveBlockCount).toBe(1);
      expect(pmSerializeBody(editor)).toContain(
        '<div class="raw">Raw HTML</div>',
      );
    } finally {
      editor.destroy();
    }
  });

  it("serializes newly appended paragraphs without amplifying blank lines", () => {
    const editor = createProductionEditorFromIngestedBody("Alpha\n\nBeta");
    try {
      editor.commands.insertContentAt(editor.state.doc.content.size, {
        type: "paragraph",
        content: [{ type: "text", text: "Gamma" }],
      });

      const md = normalizeMd(pmSerializeBody(editor));
      expect(md).toContain("Alpha");
      expect(md).toContain("Beta");
      expect(md).toContain("Gamma");
      expect(md).not.toMatch(/\n{4,}/);
    } finally {
      editor.destroy();
    }
  });

  it("ignores plain empty paragraphs on save", () => {
    const editor = createProductionEditorFromIngestedBody("Alpha");
    try {
      editor.commands.insertContentAt(editor.state.doc.content.size, {
        type: "doc",
        content: [
          { type: "paragraph" },
          { type: "paragraph", content: [{ type: "text", text: "Beta" }] },
        ],
      });

      const md = normalizeMd(pmSerializeBody(editor));
      expect(md).toBe("Alpha\n\nBeta");
      expect(md).not.toMatch(/\n{4,}/);
    } finally {
      editor.destroy();
    }
  });

  it("does not lose a list item after typing the first unordered item and pressing Enter", () => {
    const editor = createProductionEditorFromIngestedBody("");
    try {
      typeTextThroughInputRules(editor, "- one");
      pressEnter(editor);

      const md = pmSerializeBody(editor).replace(/\r\n/g, "\n");
      expect(md).toContain("- one");
      expect(md).not.toMatch(/^\s*-\s*$/m);

      const reopened = createProductionEditorFromIngestedBody(md);
      try {
        expect(reopened.getText()).toContain("one");
      } finally {
        reopened.destroy();
      }
    } finally {
      editor.destroy();
    }
  });

  it("does not lose a list item after typing the first ordered item and pressing Enter", () => {
    const editor = createProductionEditorFromIngestedBody("");
    try {
      typeTextThroughInputRules(editor, "1. one");
      pressEnter(editor);

      const md = pmSerializeBody(editor).replace(/\r\n/g, "\n");
      expect(md).toContain("1. one");
      expect(md).not.toMatch(/^\s*\d+\.\s*$/m);

      const reopened = createProductionEditorFromIngestedBody(md);
      try {
        expect(reopened.getText()).toContain("one");
      } finally {
        reopened.destroy();
      }
    } finally {
      editor.destroy();
    }
  });

  it("does not persist a trailing empty task list item created by Enter", () => {
    const editor = createProductionEditorFromIngestedBody("- [ ] one");
    try {
      editor.commands.setTextSelection(editor.state.doc.content.size - 3);
      pressEnter(editor);

      const md = pmSerializeBody(editor).replace(/\r\n/g, "\n");
      expect(md).toContain("- [ ] one");
      expect(md).not.toMatch(/^\s*-\s\[[ xX]\]\s*$/m);
    } finally {
      editor.destroy();
    }
  });

  it("falls back to Turndown when prosemirror-markdown throws", () => {
    const serializeSpy = vi
      .spyOn(MarkdownSerializer.prototype, "serialize")
      .mockImplementation(() => {
        throw new Error("unsupported node");
      });
    const turndown = vi
      .spyOn(markdownLib, "editorBodyHtmlToMarkdown")
      .mockReturnValue("turndown-body");
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    const editor = createProductionEditorFromBody("Fallback path.");
    try {
      expect(pmSerializeBody(editor)).toBe("turndown-body");
      expect(turndown).toHaveBeenCalled();
      expect(errorSpy).not.toHaveBeenCalled();
    } finally {
      serializeSpy.mockRestore();
      editor.destroy();
    }
  });
});

describe("serializeOpenNote integration (PM + ingest)", () => {
  it("preserves body heading and paragraph spaces through the full note pipeline", () => {
    const md = [
      "---",
      'title: "Whitespace"',
      "---",
      "",
      "## 第一章    总 则",
      "",
      "正文    保留  空格",
    ].join("\n");

    const out = normalizeMd(fullNoteRoundTrip(md));
    expect(out).toContain("## 第一章    总 则");
    expect(out).toContain("正文    保留  空格");
  });

  it("preserves mixed advanced syntax through full note pipeline", () => {
    const md = [
      "---",
      'title: "PM Round Trip"',
      "---",
      "",
      "See [[Target Note]].",
      "",
      "> [!warning] Heads up",
      "> Stay careful.",
      "",
      "| Col |",
      "| --- |",
      "| x |",
    ].join("\n");

    const out = normalizeMd(fullNoteRoundTrip(md));
    expect(out).toContain("[[Target Note]]");
    expect(out).toContain("[!warning]");
    expect(out).toContain("Stay careful");
    expect(out).toContain("| Col |");
  });

  it("preserves calloutType on blockquote after ingest", () => {
    const editor = createProductionEditorFromIngestedBody(
      "> [!note] Info\n> Callout body.",
    );
    try {
      let found = false;
      editor.state.doc.descendants((node) => {
        if (
          node.type.name === "blockquote" &&
          node.attrs.calloutType === "note"
        ) {
          found = true;
        }
      });
      expect(found).toBe(true);
    } finally {
      editor.destroy();
    }
  });

  it("matches production editor round-trip for links, tasks, tables, wiki-links", () => {
    const md = [
      "---",
      'title: "Round Trip"',
      "---",
      "",
      "See [Iris](https://example.com/docs).",
      "",
      "- [x] Done",
      "- [ ] Todo",
      "",
      "| A | B |",
      "| --- | --- |",
      "| 1 | 2 |",
      "",
      "See [[Architecture Notes]].",
    ].join("\n");

    const out = normalizeMd(fullNoteRoundTrip(md));
    expect(out).toContain("[Iris](https://example.com/docs)");
    expect(out).toContain("- [x] Done");
    expect(out).toContain("| A | B |");
    expect(out).toContain("[[Architecture Notes]]");
  });

  it("does not reintroduce a code-block trailing blank line through repeated full-note saves", () => {
    const md = [
      "---",
      'title: "MiMo"',
      "---",
      "",
      "## 6. 使用",
      "",
      "```bash",
      "# 一行安装，或通过 npm 安装",
      "curl -fsSL https://mimo.xiaomi.com/install | bash",
      "npm install -g @mimo-ai/cli",
      "```",
    ].join("\n");

    const first = fullNoteRoundTrip(md).replace(/\r\n/g, "\n");
    const second = fullNoteRoundTrip(first).replace(/\r\n/g, "\n");

    expect(first).toContain("npm install -g @mimo-ai/cli\n```");
    expect(second).toContain("npm install -g @mimo-ai/cli\n```");
    expect(first).not.toContain("npm install -g @mimo-ai/cli\n\n```");
    expect(second).not.toContain("npm install -g @mimo-ai/cli\n\n```");
  });
});
