import { describe, expect, it } from "vitest";

import {
  calloutMarkdownFromLines,
  detectCalloutTypeFromElement,
} from "@/lib/callout-markdown";

describe("calloutMarkdownFromLines", () => {
  it("formats title and body with Obsidian prefix", () => {
    const md = calloutMarkdownFromLines("warning", [
      "Heads up",
      "Stay careful.",
    ]);
    expect(md).toBe("> [!warning] Heads up\n> Stay careful.");
  });

  it("defaults empty type to note", () => {
    expect(calloutMarkdownFromLines("", ["Title"])).toBe("> [!note] Title");
  });
});

describe("detectCalloutTypeFromElement", () => {
  it("reads data-callout-type from ingest HTML", () => {
    const doc = new DOMParser().parseFromString(
      '<blockquote data-callout-type="tip"><p>Hint</p></blockquote>',
      "text/html",
    );
    const el = doc.querySelector("blockquote");
    expect(el).not.toBeNull();
    expect(detectCalloutTypeFromElement(el!)).toBe("tip");
  });
});
