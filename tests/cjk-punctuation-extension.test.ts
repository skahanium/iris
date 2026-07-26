import { Editor } from "@tiptap/core";
import CodeBlock from "@tiptap/extension-code-block";
import StarterKit from "@tiptap/starter-kit";
import { describe, expect, it } from "vitest";

import {
  CjkPunctuationExtension,
  cjkPunctuationPluginKey,
} from "@/components/editor/extensions/CjkPunctuationExtension";

function createEditor(enabled = true): Editor {
  const element = document.createElement("div");
  document.body.appendChild(element);
  return new Editor({
    element,
    extensions: [
      StarterKit.configure({ codeBlock: false }),
      CodeBlock,
      CjkPunctuationExtension.configure({
        isEnabled: () => enabled,
      }),
    ],
    content: "<p></p>",
  });
}

function createEditorWithRef(initialEnabled: boolean): {
  editor: Editor;
  enabledRef: { current: boolean };
} {
  const element = document.createElement("div");
  document.body.appendChild(element);
  const enabledRef = { current: initialEnabled };
  const editor = new Editor({
    element,
    extensions: [
      StarterKit.configure({ codeBlock: false }),
      CodeBlock,
      CjkPunctuationExtension.configure({
        isEnabled: () => enabledRef.current,
      }),
    ],
    content: "<p></p>",
  });
  return { editor, enabledRef };
}

function findCjkPlugin(view: Editor["view"]) {
  const targetKey = (cjkPunctuationPluginKey as unknown as { key: string }).key;
  return view.state.plugins.find(
    (p) => (p as unknown as { key: string }).key === targetKey,
  ) as
    | ((typeof view.state.plugins)[number] & {
        props: {
          handleTextInput?: (
            v: typeof view,
            from: number,
            to: number,
            t: string,
          ) => boolean;
        };
      })
    | undefined;
}

function insertAt(editor: Editor, text: string, pos?: number) {
  const insertPos = pos ?? editor.state.selection.from;
  const view = editor.view;
  const plugin = findCjkPlugin(view);
  const handler = plugin?.props.handleTextInput;
  const handled = handler ? handler(view, insertPos, insertPos, text) : false;
  if (!handled) {
    editor.commands.insertContentAt(insertPos, text);
  }
}

/** 插入文本并把光标明确放到该文本末尾，避免 insertContent 的光标不确定性。 */
function typeCjkThen(editor: Editor, base: string, atEnd = true) {
  editor.commands.setContent(`<p>${base}</p>`);
  if (atEnd) {
    editor.commands.setTextSelection(base.length + 1);
  }
}

describe("CjkPunctuationExtension handleTextInput", () => {
  it("converts . to 。 after a CJK character", () => {
    const editor = createEditor();
    try {
      typeCjkThen(editor, "你好");
      insertAt(editor, ".");
      expect(editor.state.doc.textContent).toBe("你好。");
    } finally {
      editor.destroy();
      document.body.innerHTML = "";
    }
  });

  it("leaves . untouched after a digit (protects ordered list 1.)", () => {
    const editor = createEditor();
    try {
      typeCjkThen(editor, "1");
      insertAt(editor, ".");
      expect(editor.state.doc.textContent).toBe("1.");
    } finally {
      editor.destroy();
      document.body.innerHTML = "";
    }
  });

  it("leaves . untouched after an ASCII letter (protects URLs / English)", () => {
    const editor = createEditor();
    try {
      typeCjkThen(editor, "www");
      insertAt(editor, ".");
      expect(editor.state.doc.textContent).toBe("www.");
    } finally {
      editor.destroy();
      document.body.innerHTML = "";
    }
  });

  it("converts , to ， after a CJK character", () => {
    const editor = createEditor();
    try {
      typeCjkThen(editor, "你好");
      insertAt(editor, ",");
      expect(editor.state.doc.textContent).toBe("你好，");
    } finally {
      editor.destroy();
      document.body.innerHTML = "";
    }
  });

  it("pairs smart double quotes within a CJK paragraph", () => {
    const editor = createEditor();
    try {
      typeCjkThen(editor, "他说"); // <p>他说</p>, cursor at 3
      insertAt(editor, '"'); // 他说“
      editor.commands.insertContentAt(4, "你好"); // 他说“你好
      editor.commands.setTextSelection(6); // cursor after 好
      insertAt(editor, '"'); // 他说“你好”
      expect(editor.state.doc.textContent).toBe("他说“你好”");
    } finally {
      editor.destroy();
      document.body.innerHTML = "";
    }
  });

  it("does not convert inside a code block", () => {
    const editor = createEditor();
    try {
      editor.commands.setContent("<pre><code>说</code></pre>");
      editor.commands.setTextSelection(5);
      insertAt(editor, ".");
      expect(editor.state.doc.textContent).toBe("说.");
    } finally {
      editor.destroy();
      document.body.innerHTML = "";
    }
  });

  it("does not convert when disabled", () => {
    const editor = createEditor(false);
    try {
      typeCjkThen(editor, "你好");
      insertAt(editor, ".");
      expect(editor.state.doc.textContent).toBe("你好.");
    } finally {
      editor.destroy();
      document.body.innerHTML = "";
    }
  });

  it("does not convert at block start (empty before)", () => {
    const editor = createEditor();
    try {
      editor.commands.setContent("<p></p>");
      editor.commands.setTextSelection(1);
      insertAt(editor, ".");
      expect(editor.state.doc.textContent).toBe(".");
    } finally {
      editor.destroy();
      document.body.innerHTML = "";
    }
  });

  it("respects live toggle via enabledRef without rebuilding the editor", () => {
    const { editor, enabledRef } = createEditorWithRef(true);
    try {
      typeCjkThen(editor, "你好");
      insertAt(editor, ".");
      expect(editor.state.doc.textContent).toBe("你好。");

      // 运行时关闭（模拟管理中心 toggle，不重建编辑器）
      enabledRef.current = false;
      typeCjkThen(editor, "再见");
      insertAt(editor, ".");
      expect(editor.state.doc.textContent).toBe("再见.");

      // 再次开启
      enabledRef.current = true;
      typeCjkThen(editor, "你好");
      insertAt(editor, ".");
      expect(editor.state.doc.textContent).toBe("你好。");
    } finally {
      editor.destroy();
      document.body.innerHTML = "";
    }
  });
});
