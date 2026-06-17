import { describe, expect, it } from "vitest";

import {
  buildMentionCandidates,
  findActiveMentionQuery,
  insertMentionToken,
  isFolderMention,
  normalizeFolderPrefix,
  parseMentionTokens,
  stripMentionTokensForDisplay,
  tokensToContextScope,
} from "@/lib/ai-context-scope";
import type { FileListItem } from "@/types/ipc";

const files: FileListItem[] = [
  {
    path: "党纪法规/条例.md",
    title: "条例",
    updatedAt: "2026-01-01",
    isLocked: false,
  },
  {
    path: "范文/报告/样例.md",
    title: "样例",
    updatedAt: "2026-01-01",
    isLocked: false,
  },
];

describe("ai-context-scope", () => {
  it("parses folder and file mention tokens", () => {
    const text = "请查 @[党纪法规/] 和 @[范文/报告/样例.md]";
    const tokens = parseMentionTokens(text);
    expect(tokens).toHaveLength(2);
    expect(tokens[0]?.kind).toBe("folder");
    expect(tokens[0]?.value).toBe("党纪法规/");
    expect(tokens[1]?.kind).toBe("file");
    expect(tokens[1]?.value).toBe("范文/报告/样例.md");
  });

  it("converts tokens to context scope", () => {
    const scope = tokensToContextScope(parseMentionTokens("@[党纪法规/]"));
    expect(scope.pathPrefixes).toEqual(["党纪法规/"]);
    expect(scope.paths).toEqual([]);
  });

  it("strips tokens for display", () => {
    const display = stripMentionTokensForDisplay(
      "@[问题线索工作思路（WY）.md] 根据问题线索情况，请给出核查思路",
    );
    expect(display).toBe("根据问题线索情况，请给出核查思路");
    expect(display).not.toContain("问题线索工作思路");
    expect(display).not.toContain("@[");
  });

  it("keeps readable mention metadata for Chinese document names", () => {
    const [token] = parseMentionTokens(
      "@[问题线索工作思路（WY）.md] 根据问题线索情况",
    );

    expect(token).toMatchObject({
      kind: "file",
      value: "问题线索工作思路（WY）.md",
      label: "问题线索工作思路（WY）.md",
    });
  });

  it("detects active @ query", () => {
    const text = "hello @党纪";
    const active = findActiveMentionQuery(text, text.length);
    expect(active?.query).toBe("党纪");
  });

  it("inserts mention token", () => {
    const candidate = buildMentionCandidates(files, "")[0]!;
    const { text } = insertMentionToken("问 @", 3, 2, candidate);
    expect(text).toContain("@[");
  });

  it("classifies folder vs file mentions", () => {
    expect(isFolderMention("党纪法规/")).toBe(true);
    expect(isFolderMention("note.md")).toBe(false);
    expect(normalizeFolderPrefix("a/b")).toBe("a/b/");
  });
});
