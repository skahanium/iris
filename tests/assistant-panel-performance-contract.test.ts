import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("assistant panel performance contract", () => {
  it("does not receive whole-note markdown as a render-time prop", () => {
    const panel =
      read("src/components/ai/UnifiedAssistantPanel.tsx") +
      read("src/components/ai/UnifiedAssistantPanel.impl.tsx");
    const app = read("src/App.tsx");

    expect(panel).toContain("getNoteContent: () => string");
    expect(panel).not.toContain("noteContent: string;");
    expect(app).toContain("getNoteContent={getLiveMarkdown}");
    expect(app).not.toContain("noteContent={assistantNoteContent}");
  });

  it("parses document chapters only inside explicit document/chapter tasks", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");

    expect(panel).not.toMatch(
      /useEffect\(\(\) => \{[\s\S]*parseDocumentChapters[\s\S]*\}, \[noteContent\]\)/,
    );
    expect(panel).toContain("await parseDocumentChapters(getNoteContent())");
  });

  it("keeps streamed token updates throttled and isolated from artifact surfaces", () => {
    const streamHook = read("src/hooks/useAssistantLlmStream.ts");
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    const impl = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");

    expect(streamHook).toContain(
      "const delay = elapsed < 50 ? 50 - elapsed : 0",
    );
    expect(panel).toContain("AssistantTaskSurfaces");
    expect(impl).toContain("<AssistantTaskSurfaces");
    expect(impl).not.toContain('from "./PatchPreview"');
    expect(impl).not.toContain('from "./CitationCheckView"');
    expect(impl).not.toContain('from "./assistant/DocumentCheckArtifacts"');
  });
});
