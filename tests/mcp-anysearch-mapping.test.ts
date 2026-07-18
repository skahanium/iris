import { describe, expect, it } from "vitest";

import {
  ensureAnySearchSearchMapping,
  mappingHasMaxResultsArgument,
} from "@/components/ai/skills/mcpAnySearchMapping";
import {
  credentialStateText,
  needsAnySearchResultLimitUpdate,
} from "@/components/ai/skills/mcpProfileHelpers";
import type { WebEvidenceProviderSummary } from "@/lib/ipc";

function anySearchProvider(
  searchMapping: string | null,
): WebEvidenceProviderSummary {
  return {
    id: "anysearch",
    name: "AnySearch",
    providerKind: "mcp",
    enabled: true,
    transportKind: "https",
    transportConfigJson: JSON.stringify({
      url: "https://api.anysearch.com/mcp",
    }),
    credentialRefsJson: "{}",
    searchMapping,
    fetchMapping: null,
    mappingStatus: searchMapping ? "partial" : "missing",
    diagnosticStatus: "ready",
    isNative: false,
    editable: true,
    hasSearchMapping: Boolean(searchMapping),
    hasFetchMapping: false,
  };
}

describe("AnySearch search mapping heal", () => {
  it("adds maxResultsArg when saving a legacy search mapping", () => {
    const legacy = JSON.stringify({ tool: "search", queryArg: "query" });
    const healed = ensureAnySearchSearchMapping(legacy);
    expect(healed).toContain('"maxResultsArg":"max_results"');
    expect(mappingHasMaxResultsArgument(healed)).toBe(true);
    expect(needsAnySearchResultLimitUpdate(anySearchProvider(healed))).toBe(
      false,
    );
  });

  it("marks legacy persisted mappings as needing update until healed", () => {
    expect(
      needsAnySearchResultLimitUpdate(
        anySearchProvider(JSON.stringify({ tool: "search" })),
      ),
    ).toBe(true);
  });
});

describe("MCP credential state text", () => {
  it("distinguishes bound key, anonymous optional, and pending update", () => {
    const optionalRow = {
      id: "1",
      target: "header" as const,
      name: "Authorization",
      ref: "iris.mcp.anysearch",
      optional: true,
      secretValue: "",
    };
    expect(credentialStateText([optionalRow], {})).toBe(
      "未配置 Key，将使用匿名额度",
    );
    expect(
      credentialStateText([optionalRow], { "iris.mcp.anysearch": true }),
    ).toBe("已绑定 Key，请求将携带 Bearer");
    expect(
      credentialStateText([{ ...optionalRow, secretValue: "as_sk_test" }], {
        "iris.mcp.anysearch": false,
      }),
    ).toBe("本次保存会更新 Key，请求将携带 Bearer");
  });
});
