import { describe, expect, it } from "vitest";

import {
  clearCachedEditorHtml,
  getCachedEditorHtml,
  setCachedEditorHtml,
} from "@/lib/editor-html-cache";

describe("editor-html-cache", () => {
  it("stores and retrieves html per path", () => {
    setCachedEditorHtml("a.md", "<p>x</p>");
    expect(getCachedEditorHtml("a.md")).toBe("<p>x</p>");
    clearCachedEditorHtml("a.md");
    expect(getCachedEditorHtml("a.md")).toBeUndefined();
  });
});
