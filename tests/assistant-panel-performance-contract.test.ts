import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

function lineCount(path: string): number {
  return read(path).split("\n").length;
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
    const taskHook = read("src/components/ai/hooks/useAssistantTasks.ts");

    expect(panel).not.toMatch(
      /useEffect\(\(\) => \{[\s\S]*parseDocumentChapters[\s\S]*\}, \[noteContent\]\)/,
    );
    expect(taskHook).toContain("await parseDocumentChapters(getNoteContent())");
  });

  it("keeps streamed token updates throttled and isolated from artifact surfaces", () => {
    const streamHook = read("src/hooks/useAssistantLlmStream.ts");
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    const impl = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");

    expect(streamHook).toContain("window.requestAnimationFrame");
    expect(streamHook).toContain("window.cancelAnimationFrame");
    expect(streamHook).not.toContain(
      "const delay = elapsed < 50 ? 50 - elapsed : 0",
    );
    expect(streamHook).not.toContain("window.setTimeout");
    expect(panel).toContain("AssistantTaskSurfaces");
    expect(impl).toContain("<AssistantTaskSurfaces");
    expect(impl).not.toContain('from "./PatchPreview"');
    expect(impl).not.toContain('from "./CitationCheckView"');
    expect(impl).not.toContain('from "./assistant/DocumentCheckArtifacts"');
  });

  it("moves assistant task orchestration behind a dedicated hook", () => {
    const taskHook = read("src/components/ai/hooks/useAssistantTasks.ts");
    const impl = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");

    expect(taskHook).toContain("executeKnowledgeChat");
    expect(taskHook).toContain("runKnowledgeChat");
    expect(taskHook).toContain("runWriting");
    expect(taskHook).toContain("runCitation");
    expect(taskHook).toContain("runOrganize");
    expect(taskHook).toContain("runChapter");
    expect(taskHook).toContain("runDocumentCheck");
    expect(taskHook).toContain("runResearch");
    expect(taskHook).toContain("send");
    expect(impl).not.toContain("const executeKnowledgeChat");
    expect(impl).not.toContain("const runKnowledgeChat");
    expect(impl).not.toContain("const runWriting");
    expect(impl).not.toContain("const runCitation");
    expect(impl).not.toContain("const runOrganize");
    expect(impl).not.toContain("const runChapter");
    expect(impl).not.toContain("const runDocumentCheck");
    expect(impl).not.toContain("const runResearch");
  });

  it("keeps the assistant panel below the current refactor checkpoint", () => {
    // 547: current post-split assistant checkpoint after extracting task
    // orchestration, research control, confirmations, and artifact surfaces.
    expect(
      lineCount("src/components/ai/UnifiedAssistantPanel.impl.tsx"),
    ).toBeLessThanOrEqual(547);
  });

  it("moves research control behind a dedicated hook", () => {
    const researchHook = read("src/components/ai/hooks/useResearchControl.ts");
    const impl = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");

    expect(researchHook).toContain("listenResearchProgress");
    expect(researchHook).toContain("abortResearch");
    expect(researchHook).toContain("handleGenerateResearchNote");
    expect(researchHook).toContain("handleExpandResearchDetail");
    expect(impl).not.toContain("listenResearchProgress");
    expect(impl).not.toContain("const abortResearch");
    expect(impl).not.toContain("const handleGenerateResearchNote");
    expect(impl).not.toContain("const handleExpandResearchDetail");
  });

  it("virtualizes long assistant conversations with the existing virtualizer", () => {
    const list = read("src/components/ai/AiMessageList.tsx");

    expect(list).toContain("@tanstack/react-virtual");
    expect(list).toContain("useVirtualizer");
  });
});
