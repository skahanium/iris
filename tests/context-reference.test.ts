import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import {
  contextReferenceDisplayText,
  createContextReference,
  validateContextReference,
} from "@/lib/context-reference";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("context references", () => {
  it("preserves an irregular partial selection range", () => {
    const content = "第一段：甲乙丙丁。";
    const start = new TextEncoder().encode("第一段：甲").length;
    const end = new TextEncoder().encode("第一段：甲乙丙").length;

    const reference = createContextReference({
      kind: "selection",
      filePath: "notes/a.md",
      content,
      utf8Range: { start, end },
      editorRange: { from: 4, to: 7 },
    });

    expect(reference.utf8Range).toEqual({ start, end });
    expect(reference.editorRange).toEqual({ from: 4, to: 7 });
    expect(reference.excerpt).toBe("乙丙");
  });

  it("preserves a cross-paragraph selection without expanding to full paragraphs", () => {
    const content = "第一段开头\n第二段中间\n第三段结尾";
    const start = new TextEncoder().encode("第一段开").length;
    const end = new TextEncoder().encode("第一段开头\n第二段中").length;

    const reference = createContextReference({
      kind: "selection",
      filePath: "notes/a.md",
      content,
      utf8Range: { start, end },
      editorRange: { from: 3, to: 10 },
    });

    expect(reference.excerpt).toBe("头\n第二段中");
    expect(reference.excerpt).not.toContain("第一段开");
    expect(reference.excerpt).not.toContain("间\n第三段");
  });

  it("marks a reference stale when content hash changes", () => {
    const reference = createContextReference({
      kind: "selection",
      filePath: "notes/a.md",
      content: "原始内容",
      utf8Range: null,
      editorRange: null,
    });

    const checked = validateContextReference(reference, "修改后的内容");

    expect(checked.stale).toBe(true);
    expect(checked.invalidReason).toBe("content_changed");
  });

  it("creates a lightweight display capsule without dumping the whole source text", () => {
    const content = "很长的选区".repeat(30);
    const reference = createContextReference({
      kind: "selection",
      filePath: "/vault/projects/long-note.md",
      content,
      utf8Range: null,
      editorRange: null,
    });

    const display = contextReferenceDisplayText(reference);

    expect(display).toContain("long-note.md");
    expect(display.length).toBeLessThan(120);
    expect(display).not.toContain(content);
  });

  it("serializes references through assistantExecute", () => {
    const tasks = read("src/components/ai/hooks/useAssistantTasks.ts");

    expect(tasks).toContain("contextReferences: currentContextReferences()");
    expect(tasks).toContain("activeContextReferences.length > 0");
    expect(tasks).not.toContain("contextReferences: []");
  });
});
