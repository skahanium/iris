import { describe, expect, it } from "vitest";

import {
  buildAiSystemParts,
  filterRelatedSemanticHits,
  formatRelatedNotesSection,
} from "@/lib/ai-context";
import type { SemanticHit } from "@/types/ipc";

function hit(
  path: string,
  score: number,
  snippet = "snippet",
): SemanticHit {
  return {
    chunk_id: 1,
    path,
    title: path.replace(/\.md$/, ""),
    snippet,
    score,
  };
}

describe("filterRelatedSemanticHits", () => {
  it("excludes current note and dedupes by path", () => {
    const raw = [
      hit("current.md", 0.9),
      hit("other-a.md", 0.8),
      hit("other-a.md", 0.7),
      hit("other-b.md", 0.6),
    ];
    const filtered = filterRelatedSemanticHits(raw, "current.md", 5);
    expect(filtered.map((h) => h.path)).toEqual(["other-a.md", "other-b.md"]);
    expect(filtered[0]?.score).toBe(0.8);
  });

  it("returns empty when only current note matches", () => {
    const filtered = filterRelatedSemanticHits(
      [hit("note.md", 0.9)],
      "note.md",
    );
    expect(filtered).toEqual([]);
  });
});

describe("buildAiSystemParts", () => {
  it("includes related section when hits present", () => {
    const parts = buildAiSystemParts({
      notePath: "a.md",
      noteContent: "body",
      quote: null,
      relatedHits: [hit("b.md", 0.85, "related text")],
    });
    const joined = parts.join("\n");
    expect(joined).toContain("当前笔记 (a.md)");
    expect(joined).toContain("关联 1");
    expect(joined).toContain("b.md");
    expect(joined).toContain("related text");
  });

  it("degrades to current note only without related hits", () => {
    const parts = buildAiSystemParts({
      notePath: "a.md",
      noteContent: "only",
      quote: null,
      relatedHits: [],
    });
    expect(parts.join("\n")).toContain("当前笔记");
    expect(formatRelatedNotesSection([])).toBeNull();
  });
});
