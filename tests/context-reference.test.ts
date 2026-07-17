import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { contextReferenceDisplayText } from "@/lib/context-reference";
import type { ContextReference } from "@/types/ai";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("context references", () => {
  it("creates a lightweight display capsule without dumping the whole source text", () => {
    const content = "很长的选区".repeat(30);
    const reference = {
      id: "selection:notes/long-note.md:hash:0:12",
      kind: "selection",
      filePath: "/vault/projects/long-note.md",
      contentHash: "a".repeat(64),
      utf8Range: { start: 0, end: 12 },
      editorRange: { from: 1, to: 5 },
      excerpt: "选区摘要",
      stale: false,
      invalidReason: null,
    } satisfies ContextReference;

    const display = contextReferenceDisplayText(reference);

    expect(display).toContain("long-note.md");
    expect(display.length).toBeLessThan(120);
    expect(display).not.toContain(content);
  });

  it("does not keep active-editor text in sidecar routing or selection bridge state", () => {
    const routing = read("src/hooks/useWorkspaceAssistantRouting.ts");
    const bridge = read("src/hooks/useAiSidecarBridge.ts");

    expect(routing).not.toContain("getLiveMarkdown");
    expect(routing).not.toContain("getTabMarkdownCached");
    expect(routing).not.toContain("RuntimeDocumentSnapshot");
    expect(bridge).not.toContain("getNoteContent");
    expect(bridge).not.toContain("content: classifiedSelection");
    expect(bridge).toContain("createEditorContextReference");
    expect(bridge).not.toContain("getEditorSelectionSnapshot");
  });
  it("serializes references through the unified Run sender", () => {
    const sender = read("src/components/ai/hooks/useUnifiedAssistantSend.ts");
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");

    expect(sender).toContain("explicitReferences");
    expect(sender).toContain("contextReferences.filter");
    expect(sender).toContain("!reference.stale && !reference.invalidReason");
    expect(panel).toContain("bubbleSelection.contextReferences");
    expect(sender).not.toContain("assistantExecute");
  });
});
