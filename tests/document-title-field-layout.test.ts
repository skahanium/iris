import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("document title field layout", () => {
  it("uses a multiline title control without allowing newline titles", () => {
    const source = read("src/components/editor/DocumentTitleField.tsx");

    expect(source).toContain("<textarea");
    expect(source).toContain("rows={1}");
    expect(source).toContain('data-testid="document-title"');
    expect(source).toContain("event.preventDefault()");
    expect(source).toContain("sanitizeDocumentTitleInput");
    expect(source).toContain("onBlur?.(next)");
  });

  it("clamps long titles to three lines and expands while focused", () => {
    const css = read("src/styles/globals.css");

    expect(css).toContain("--doc-title-line-height: 1.2");
    expect(css).toContain("--doc-title-max-lines: 3");
    expect(css).toContain("--doc-title-focus-max-lines: 6");
    expect(css).toContain("overflow-wrap: anywhere");
    expect(css).toContain("max-height: calc(");
    expect(css).toContain(
      ".iris-document-title-field:focus-within .iris-doc-title",
    );
  });
});
