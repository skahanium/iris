import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("knowledge relations panel contract", () => {
  it("combines backlinks and tags into one task overlay", () => {
    const panel = read("src/components/knowledge/KnowledgeRelationsPanel.tsx");
    const overlays = read("src/components/layout/AppOverlays.tsx");
    const manager = read("src/hooks/useOverlayManager.ts");
    const palette = read("src/lib/command-palette.ts");

    expect(panel).toContain('data-testid="knowledge-relations-panel"');
    expect(panel).toContain('data-testid="knowledge-relations-tab-backlinks"');
    expect(panel).toContain('data-testid="knowledge-relations-tab-tags"');
    expect(panel).toContain('title="知识关联"');
    expect(panel).toContain('size="command"');
    expect(panel).toContain("fileBacklinks");
    expect(panel).toContain("tagList");
    expect(panel).toContain("反向链接");
    expect(panel).toContain("标签");

    expect(overlays).toContain("KnowledgeRelationsPanel");
    expect(overlays).not.toContain("BacklinksPanel");
    expect(overlays).not.toContain("TagView");
    expect(manager).toContain('"knowledgeRelations"');
    expect(manager).not.toContain('"backlinks"');
    expect(manager).not.toContain('"tags"');
    expect(palette).toContain("knowledge-relations");
    expect(palette).not.toContain('id: "backlinks"');
    expect(palette).not.toContain('id: "tags"');
  });
});
