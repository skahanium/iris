import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return "";
  }
}

describe("writing and research artifact surfaces", () => {
  it("does not keep legacy writing or research state cards in the sidecar", () => {
    expect(read("src/components/ai/assistant/WritingStatePanel.tsx")).toBe("");
    expect(read("src/components/ai/assistant/ResearchStatePanel.tsx")).toBe("");
    expect(read("src/components/ai/AssistantTaskSurfaces.tsx")).not.toContain(
      "WritingStatePanel",
    );
    expect(read("src/components/ai/AssistantTaskSurfaces.tsx")).not.toContain(
      "ResearchStatePanel",
    );
  });

  it("keeps writing and research details in readonly artifact workspace views", () => {
    const workspace = read("src/components/layout/ArtifactWorkspaceView.tsx");

    expect(workspace).toContain("WritingChangeArtifactView");
    expect(workspace).toContain("EvidenceSourcesArtifactView");
    expect(workspace).toContain("证据矩阵");
    expect(workspace).toContain("证据缺口");
    expect(workspace).toContain("写作修改");
    expect(workspace).toContain("接受修改");
    expect(workspace).not.toContain("raw_web_page");
    expect(workspace).not.toContain("full_note_content");
  });
});
