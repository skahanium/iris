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
      reconcileDisplayMentions(
        "查 Guide 然后继续",
        "请先查 Guide 然后继续",
        [guideMention],
        { from: 0, to: 0, insertedTextLength: 2 },
      ),
    ).toEqual([
      {
        ...guideMention,
        range: { from: 4, to: 9 },
      },
    ]);
  });

  it("uses the exact edit transaction when repeated text makes a prefix diff ambiguous", () => {
    const secondGuide: DisplayMention = {
      ...guideMention,
      range: { from: 6, to: 11 },
    };

    expect(
      reconcileDisplayMentions(
        "Guide Guide",
        "Guide Guide Guide",
        [secondGuide],
        { from: 0, to: 0, insertedTextLength: 6 },
      ),
    ).toEqual([
      {
        ...secondGuide,
        range: { from: 12, to: 17 },
      },
    ]);
  });

  it("keeps matching mentions when the edit transaction cannot explain the text change", () => {
    expect(
      reconcileDisplayMentions(
        "查 Guide 然后继续",
        "改 Guide 然后继续",
        [guideMention],
        { from: 12, to: 12, insertedTextLength: 0 },
      ),
    ).toEqual([guideMention]);
  });

  it("keeps Chinese fullwidth-parenthesis mentions when typing continues after them", () => {
    const label = "问题线索工作思路（刘CG）";
    const previous = `根据 ${label}`;
    const from = previous.indexOf(label);
    const mention: DisplayMention = {
      kind: "file",
      value: "线索/问题线索工作思路（刘CG）.md",
      label,
      range: { from, to: from + label.length },
    };
    const suffix = "，我们应该怎样分析刘CG的责任？";
    const next = `${previous}${suffix}`;

    expect(
      reconcileDisplayMentions(previous, next, [mention], {
        from: previous.length,
        to: previous.length,
        insertedTextLength: suffix.length,
      }),
    ).toEqual([mention]);

    expect(reconcileDisplayMentions(previous, next, [mention])).toEqual([
      mention,
    ]);
  });

  it.each([
    ["输入", "查 GuXide 然后继续", { from: 4, to: 4, insertedTextLength: 1 }],
    ["删除", "查 Gude 然后继续", { from: 4, to: 5, insertedTextLength: 0 }],
    [
      "粘贴",
      "查 Gu粘贴ide 然后继续",
      { from: 4, to: 4, insertedTextLength: 2 },
    ],
    ["IME 组合", "查 指南 然后继续", { from: 2, to: 7, insertedTextLength: 2 }],
  ])(
    "unbinds a mention when %s touches its displayed label",
    (_kind, next, edit) => {
      expect(
        reconcileDisplayMentions(
          "查 Guide 然后继续",
          next,
          [guideMention],
          edit,
        ),
      ).toEqual([]);
    },
  );

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
