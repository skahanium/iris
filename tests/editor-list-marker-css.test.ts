import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

const css = readFileSync(resolve("src/styles/globals.css"), "utf8");

describe("editor list marker CSS", () => {
  it("restores markers and padding for ordinary nested lists", () => {
    expect(css).toContain(".iris-editor-body .ProseMirror :where(ul, ol)");
    expect(css).toContain("list-style-position: outside");
    expect(css).toContain(".iris-editor-body .ProseMirror ul ul");
    expect(css).toContain("list-style-type: circle");
    expect(css).toContain(".iris-editor-body .ProseMirror ol ol");
    expect(css).toContain("list-style-type: lower-alpha");
  });

  it("keeps task lists markerless", () => {
    expect(css).toContain(
      '.iris-editor-body .ProseMirror ul[data-type="taskList"]',
    );
    expect(css).toContain("list-style: none");
  });
});
