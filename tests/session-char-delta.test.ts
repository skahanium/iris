import { describe, expect, it } from "vitest";

import { sessionCharDeltaFromTransaction } from "@/lib/session-char-delta";

import { createProductionEditorFromBody } from "./helpers/tiptap-serialize-harness";

describe("sessionCharDeltaFromTransaction", () => {
  it("returns zero for empty transaction", () => {
    const editor = createProductionEditorFromBody("hello world");
    try {
      const tr = editor.state.tr;
      expect(sessionCharDeltaFromTransaction(tr)).toEqual({
        added: 0,
        removed: 0,
      });
    } finally {
      editor.destroy();
    }
  });

  it("counts pure insertion the same at different positions", () => {
    const insert = "XYZ";
    const baseline = "aaaa bbbb cccc";

    const editor = createProductionEditorFromBody(baseline);
    try {
      const atEnd = editor.state.tr.insertText(
        insert,
        editor.state.doc.content.size,
      );
      const endDelta = sessionCharDeltaFromTransaction(atEnd);

      const midPos = 6;
      const atMid = editor.state.tr.insertText(insert, midPos);
      const midDelta = sessionCharDeltaFromTransaction(atMid);

      const atStart = editor.state.tr.insertText(insert, 1);
      const startDelta = sessionCharDeltaFromTransaction(atStart);

      expect(endDelta).toEqual({ added: 3, removed: 0 });
      expect(midDelta).toEqual(endDelta);
      expect(startDelta).toEqual(endDelta);
    } finally {
      editor.destroy();
    }
  });

  it("counts deletion", () => {
    const editor = createProductionEditorFromBody("remove me");
    try {
      const tr = editor.state.tr.delete(1, 7);
      expect(sessionCharDeltaFromTransaction(tr)).toEqual({
        added: 0,
        removed: 6,
      });
    } finally {
      editor.destroy();
    }
  });

  it("counts replacement as add and remove", () => {
    const editor = createProductionEditorFromBody("alpha");
    try {
      const tr = editor.state.tr.insertText("beta", 1, 6);
      expect(sessionCharDeltaFromTransaction(tr)).toEqual({
        added: 4,
        removed: 5,
      });
    } finally {
      editor.destroy();
    }
  });

  it("ignores whitespace in inserted and deleted slices", () => {
    const editor = createProductionEditorFromBody("ab");
    try {
      const tr = editor.state.tr.insertText(" c d ", 3);
      expect(sessionCharDeltaFromTransaction(tr)).toEqual({
        added: 2,
        removed: 0,
      });
    } finally {
      editor.destroy();
    }
  });

  it("accumulates multiple steps in one transaction", () => {
    const editor = createProductionEditorFromBody("aa");
    try {
      let tr = editor.state.tr.insertText("b", 2);
      tr = tr.insertText("c", tr.doc.content.size);
      expect(sessionCharDeltaFromTransaction(tr)).toEqual({
        added: 2,
        removed: 0,
      });
    } finally {
      editor.destroy();
    }
  });

  it("counts delete that undoes a prior insert as removed only", () => {
    const editor = createProductionEditorFromBody("hello");
    try {
      const insertTr = editor.state.tr.insertText("!", 6);
      editor.view.dispatch(insertTr);
      const delTr = editor.state.tr.delete(6, 7);
      expect(sessionCharDeltaFromTransaction(insertTr)).toEqual({
        added: 1,
        removed: 0,
      });
      expect(sessionCharDeltaFromTransaction(delTr)).toEqual({
        added: 0,
        removed: 1,
      });
    } finally {
      editor.destroy();
    }
  });
});
