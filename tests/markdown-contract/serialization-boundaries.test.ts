import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("markdown serialization path boundaries", () => {
  it("production note saving uses the PM serializer, not the deprecated contract export", () => {
    const source = read("src/lib/serialize-open-note.ts");

    expect(source).toContain('from "@/lib/editor-pm-serialize"');
    expect(source).not.toContain('from "@/lib/editor-export"');
    expect(source).not.toContain("exportEditorToMarkdown(");
  });

  it("deprecated HTML fragment export is explicitly marked contract-only", () => {
    const source = read("src/lib/editor-export.ts");

    expect(source).toContain("@deprecated");
    expect(source).toContain("EDITOR_EXPORT_CONTRACT_ONLY");
    expect(source).toContain("contract tests");
  });

  it("contract classification delegates safety and source reconciliation to focused modules", () => {
    const source = read("src/lib/markdown-contract/contract.ts");

    expect(source).toContain('from "./html-safety"');
    expect(source).toContain('from "./fragment-reconcile"');
    expect(source).not.toContain("function isDangerousHtml");
    expect(source).not.toContain("function reconcileFragmentsWithSource");
  });
});
