import { describe, expect, it } from "vitest";

import {
  buildMentionCandidates,
  findActiveMentionQuery,
  insertDisplayMention,
  mentionsToContextScope,
  reconcileDisplayMentions,
} from "@/lib/ai-context-scope";
import type { DisplayMention } from "@/types/ai";
import type { FileListItem, TagGroup } from "@/types/ipc";

const files: FileListItem[] = [
  {
    path: "Policies/Guide.md",
    title: "Guide",
    updatedAt: "2026-01-01",
    isLocked: false,
  },
  {
    path: "Research/Guide.md",
    title: "Guide",
    updatedAt: "2026-01-01",
    isLocked: false,
  },
  {
    path: "Research/Notes/Alpha.md",
    title: "Alpha",
    updatedAt: "2026-01-01",
    isLocked: false,
  },
];

const tags: TagGroup[] = [{ name: "project", files: [files[0]!] }];

const guideMention: DisplayMention = {
  kind: "file",
  value: "Policies/Guide.md",
  label: "Guide",
  range: { from: 2, to: 7 },
};

describe("ai-context-scope", () => {
  it("builds readable file and leaf-folder candidates while retaining relative-path subtitles", () => {
    const candidates = buildMentionCandidates(files, "", {
      prefix: "@",
      tags,
    });
    const duplicateGuides = candidates.filter(
      (candidate) => candidate.kind === "file" && candidate.label === "Guide",
    );
    const notesFolder = candidates.find(
      (candidate) => candidate.value === "Research/Notes/",
    );

    expect(duplicateGuides.map((candidate) => candidate.subtitle)).toEqual([
      "Policies/Guide.md",
      "Research/Guide.md",
    ]);
    expect(notesFolder).toMatchObject({
      kind: "folder",
      label: "Notes",
      subtitle: "Research/Notes/",
    });
  });

  it("builds tag candidates only for a # query", () => {
    expect(buildMentionCandidates(files, "pro", { prefix: "#", tags })).toEqual(
      [
        {
          id: "tag:project",
          kind: "tag",
          label: "project",
          value: "project",
        },
      ],
    );
  });

  it("inserts only the readable label and returns a positional annotation", () => {
    const candidate = buildMentionCandidates(files, "Guid", {
      prefix: "@",
      tags,
    }).find((item) => item.value === "Policies/Guide.md")!;

    const result = insertDisplayMention("问 @Guid", 7, 2, candidate);

    expect(result.text).toBe("问 Guide ");
    expect(result.text).not.toMatch(/@|\[|\]/);
    expect(result.mention).toEqual({
      kind: "file",
      value: "Policies/Guide.md",
      label: "Guide",
      range: { from: 2, to: 7 },
    });
    expect(
      result.text.slice(result.mention.range.from, result.mention.range.to),
    ).toBe(result.mention.label);
  });

  it("shifts an annotation when text is inserted before it", () => {
    expect(
      reconcileDisplayMentions("查 Guide 然后继续", "请先查 Guide 然后继续", [
        guideMention,
      ]),
    ).toEqual([
      {
        ...guideMention,
        range: { from: 4, to: 9 },
      },
    ]);
  });

  it.each([
    ["输入", "查 GuXide 然后继续"],
    ["删除", "查 Gude 然后继续"],
    ["粘贴", "查 Gu粘贴ide 然后继续"],
    ["IME 组合", "查 指南 然后继续"],
  ])("unbinds a mention when %s touches its displayed label", (_kind, next) => {
    expect(
      reconcileDisplayMentions("查 Guide 然后继续", next, [guideMention]),
    ).toEqual([]);
  });

  it("drops stale annotations whose range no longer matches the readable label", () => {
    expect(
      reconcileDisplayMentions("查 Guide", "查 Guide", [
        { ...guideMention, label: "Other" },
      ]),
    ).toEqual([]);
  });

  it("maps folders and tags to retrieval scope but excludes file mentions", () => {
    expect(
      mentionsToContextScope([
        guideMention,
        {
          kind: "folder",
          value: "Research/Notes/",
          label: "Notes",
          range: { from: 8, to: 13 },
        },
        {
          kind: "tag",
          value: "project",
          label: "project",
          range: { from: 14, to: 21 },
        },
      ]),
    ).toEqual({
      paths: [],
      pathPrefixes: ["Research/Notes/"],
      requiredTags: ["project"],
    });
  });

  it("detects active @ and # queries without treating selected labels as encoded tokens", () => {
    expect(findActiveMentionQuery("hello @Pol", 10)).toEqual({
      start: 6,
      query: "Pol",
      prefix: "@",
    });
    expect(findActiveMentionQuery("hello #pro", 10)).toEqual({
      start: 6,
      query: "pro",
      prefix: "#",
    });
    expect(findActiveMentionQuery("hello Guide", 11)).toBeNull();
  });
});
