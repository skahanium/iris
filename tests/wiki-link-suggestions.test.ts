import { describe, expect, it } from "vitest";

import {
  buildWikiLinkSuggestionItems,
  filterWikiLinkSuggestionItems,
  findWikiLinkSuggestionMatch,
} from "@/lib/wiki-link-suggestions";
import type { FileListItem } from "@/types/ipc";

const files: FileListItem[] = [
  {
    path: "Projects/SkillHub 安装说明.md",
    title: "SkillHub 安装说明",
    updatedAt: "2026-06-17T00:00:00Z",
    isLocked: false,
  },
  {
    path: "Archive/Iris 双链设计.md",
    title: "Iris 双链设计",
    updatedAt: "2026-06-16T00:00:00Z",
    isLocked: false,
  },
];

describe("wiki link suggestions", () => {
  it("matches the ASCII wiki-link trigger and query before the cursor", () => {
    expect(findWikiLinkSuggestionMatch("参考 [[Skill")).toEqual({
      index: 3,
      query: "Skill",
      text: "[[Skill",
      trigger: "[[",
    });
  });

  it("matches the full-width wiki-link trigger", () => {
    expect(findWikiLinkSuggestionMatch("参考 【【双链")).toEqual({
      index: 3,
      query: "双链",
      text: "【【双链",
      trigger: "【【",
    });
  });

  it("does not match text after a closed wiki link", () => {
    expect(findWikiLinkSuggestionMatch("参考 [[SkillHub]]")).toBeNull();
    expect(findWikiLinkSuggestionMatch("参考 【【SkillHub】】")).toBeNull();
  });

  it("filters notes by title and path keywords", () => {
    const items = buildWikiLinkSuggestionItems(files);

    expect(filterWikiLinkSuggestionItems(items, "skill")[0]?.title).toBe(
      "SkillHub 安装说明",
    );
    expect(filterWikiLinkSuggestionItems(items, "Archive")[0]?.title).toBe(
      "Iris 双链设计",
    );
  });

  it("limits the visible suggestions", () => {
    const items = buildWikiLinkSuggestionItems(files);

    expect(filterWikiLinkSuggestionItems(items, "", 1)).toHaveLength(1);
  });
});
