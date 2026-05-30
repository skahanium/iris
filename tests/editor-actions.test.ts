import { describe, expect, it } from "vitest";

import {
  editorActionById,
  filterEditorActions,
  isEditorActionEnabled,
  slashMenuActions,
} from "@/lib/editor-actions";
import { buildSlashItemsFromContext } from "@/lib/slash-commands";

describe("editor-actions", () => {
  const withSelection = {
    hasNote: true,
    hasSelection: true,
    streaming: false,
  };
  const documentOnly = {
    hasNote: true,
    hasSelection: false,
    streaming: false,
  };

  it("context menu includes selection AI when text is selected", () => {
    const ids = filterEditorActions(
      "context_menu",
      "editor",
      withSelection,
    ).map((a) => a.id);
    expect(ids).toContain("rewrite");
    expect(ids).toContain("send-to-ai");
    expect(ids).toContain("cite");
    expect(ids).toContain("check");
  });

  it("slash menu hides selection AI when text is selected", () => {
    const ids = slashMenuActions(withSelection).map((a) => a.id);
    expect(ids).not.toContain("rewrite");
    expect(ids).not.toContain("send-to-ai");
    expect(ids).toContain("summarize");
  });

  it("slash menu shows document commands without selection", () => {
    const ids = slashMenuActions(documentOnly).map((a) => a.id);
    expect(ids).toContain("summarize");
    expect(ids).not.toContain("send-to-ai");
  });

  it("context menu shows clipboard in editor scope", () => {
    const ids = filterEditorActions("context_menu", "editor", documentOnly).map(
      (a) => a.id,
    );
    expect(ids).toContain("copy");
    expect(ids).toContain("paste");
  });

  it("disables inline AI while streaming", () => {
    const rewrite = editorActionById("rewrite");
    expect(rewrite).toBeDefined();
    expect(
      isEditorActionEnabled(rewrite!, { ...withSelection, streaming: true }),
    ).toBe(false);
  });

  it("buildSlashItemsFromContext maps labels", () => {
    const items = buildSlashItemsFromContext(documentOnly);
    expect(items.some((i) => i.id === "summarize" && i.label === "总结")).toBe(
      true,
    );
  });

  it("ai_message context menu has copy and quote", () => {
    const items = filterEditorActions("context_menu", "ai_message", {
      hasNote: true,
      hasSelection: true,
      streaming: false,
    }).map((a) => a.id);
    expect(items).toContain("copy");
    expect(items).toContain("quote-to-input");
  });
});
