import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("document title field layout", () => {
  it("uses an uncontrolled title textarea reset by note session", () => {
    const source = read("src/components/editor/DocumentTitleField.tsx");
    const app = read("src/App.impl.tsx");

    expect(source).toContain("<textarea");
    expect(source).toContain("rows={1}");
    expect(source).toContain('data-testid="document-title"');
    expect(source).toContain("defaultValue={value}");
    expect(source).toContain("resetKey: string");
    expect(source).toContain("focusedRef");
    expect(source).toContain("if (el.value !== value)");
    expect(source).toContain("event.preventDefault()");
    expect(source).toContain("sanitizeDocumentTitleInput");
    expect(source).toContain("onBlur?.(next)");
    expect(source).not.toMatch(/value=\{value\}/);
    expect(app).toContain(
      'resetKey={activeDocumentSessionId ?? activePath ?? ""}',
    );
  });

  it("clamps long titles and keeps focus geometry stable", () => {
    const css = read("src/styles/globals.css");

    expect(css).toContain("--doc-title-line-height: 1.2");
    expect(css).toContain("--doc-title-max-lines: 3");
    expect(css).toContain("--doc-title-focus-max-lines: 6");
    expect(css).toContain("overflow-wrap: anywhere");
    expect(css).toContain("max-height: calc(");
    expect(css).toContain("text-center font-bold text-editor-ink");
    expect(css).not.toContain(
      ".iris-document-title-field:focus-within .iris-doc-title",
    );
  });
});
