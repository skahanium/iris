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
    updated_at: "2026-01-01",
  },
  {
    path: "范文/报告/样例.md",
    title: "样例",
    updated_at: "2026-01-01",
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
    const display = stripMentionTokensForDisplay("参考 @[党纪法规/某某.md]");
    expect(display).toContain("党纪法规/某某.md");
    expect(display).not.toContain("@[");
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
