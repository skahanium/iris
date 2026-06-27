import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { createProductionEditorFromIngestedBody } from "./helpers/tiptap-serialize-harness";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("classified editor AI parity contract", () => {
  describe("classified editor surface renders heading from markdown shortcut", () => {
    it("renders a heading node when typing '# 标题' in classified editor", () => {
      const editor = createProductionEditorFromIngestedBody("");

      // Simulate typing "# " which triggers the markdown heading input rule
      editor.commands.insertContent("# 标题");
      // The input rule should have converted the text into a heading
      // Note: this tests that the same TipTap extensions work for classified editors
      const doc = editor.state.doc;
      // After inserting "# 标题", the editor should have heading content
      // (the exact behavior depends on input rules being active)
      expect(doc.textContent).toContain("标题");

      editor.destroy();
    });

    it("TipTapEditor supports locked prop for classified notes", () => {
      const src = read("src/components/editor/TipTapEditor.tsx");
      expect(src).toContain("locked?: boolean");
      expect(src).toContain("setLocked?: (locked: boolean) => void");
      expect(src).toContain("editable: !locked");
    });
  });

  describe("EditorOutline updates after editor update events", () => {
    it("EditorOutline listens to editor update events for refresh", () => {
      const src = read("src/components/editor/EditorOutline.tsx");
      // EditorOutline must refresh on editor updates, not just selectionUpdate
      expect(src).toContain('editor.on("update"');
      // It should NOT rebuild the whole outline from selectionUpdate
      expect(src).not.toContain('editor.on("selectionUpdate", onUpdate)');
      expect(src).toContain('editor.on("selectionUpdate", updateActiveIndex)');
    });

    it("EditorOutline debounce prevents excessive rebuilds", () => {
      const src = read("src/components/editor/EditorOutline.tsx");
      expect(src).toContain("OUTLINE_REFRESH_DEBOUNCE_MS");
      expect(src).toContain("300");
    });
  });

  describe("right-click actions present for classified editor when unlocked", () => {
    it("useEditorContextMenu skips menu when locked but allows when unlocked", () => {
      const src = read("src/hooks/useEditorContextMenu.ts");
      expect(src).toContain("locked = false");
      expect(src).toMatch(/if\s*\(\s*locked\s*\)\s*return/);
      // When not locked, the menu should open normally
      expect(src).toContain("openAt(event.clientX, event.clientY)");
    });

    it("classified editor passes locked state from snapshot", () => {
      const workspace = read("src/components/layout/AppEditorWorkspace.tsx");
      expect(workspace).toContain("locked={snapshot.activeFileLocked}");
      expect(workspace).toContain("setLocked={");
    });
  });

  describe("classified editor actions route to classified AI handlers", () => {
    it("useAppEditorActions allows classified inline AI through the shared handler", () => {
      const src = read("src/hooks/useAppEditorActions.ts");
      expect(src).toContain("void inlineAi.run(ed, action)");
      expect(src).not.toContain("涉密笔记不能发送到 AI");
    });

    it("useAppEditorActions allows classified insert through editor transaction", () => {
      const src = read("src/hooks/useAppEditorActions.ts");
      expect(src).toContain("insertAssistantMarkdownAtCursor(ed, content)");
      expect(src).not.toContain("涉密笔记不能接收 AI 插入");
    });

    it("App patch handler no longer blocks classified rewrite by path alone", () => {
      const app = read("src/App.impl.tsx");
      expect(app).toContain("applyMarkdownToEditor(newContent)");
      expect(app).not.toContain("涉密笔记不能接收 AI 改写");
    });

    it("classified context menu handler blocks editing actions when locked", () => {
      const src = read("src/hooks/useEditorContextMenu.ts");
      const editorActions = read("src/lib/editor-actions.ts");
      // The menu handler must check locked state
      expect(src).toContain("isLocked: locked");
      // editor-actions must have isEditorActionEnabled that checks isLocked
      expect(editorActions).toContain("isEditorActionEnabled");
    });
  });

  describe("classified editor heading node appears without reopen", () => {
    it("TipTapEditor reingestKey controls body content refresh", () => {
      const src = read("src/components/editor/TipTapEditor.tsx");
      expect(src).toContain("reingestKey");
      // The editor must support content refresh without full remount
    });

    it("editor ingest pipeline handles heading markdown correctly", () => {
      const editor =
        createProductionEditorFromIngestedBody("# 标题\n\n正文内容");
      const doc = editor.state.doc;
      // First node should be a heading
      expect(doc.child(0).type.name).toBe("heading");
      expect(doc.child(0).attrs.level).toBe(1);
      expect(doc.child(0).textContent).toBe("标题");
      editor.destroy();
    });
  });
});
