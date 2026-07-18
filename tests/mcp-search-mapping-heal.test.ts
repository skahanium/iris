import { describe, expect, it } from "vitest";

import {
  credentialStateText,
  needsSearchResultLimitUpdate,
} from "@/components/ai/skills/mcpProfileHelpers";
import {
  ensureProviderSearchMappingResultLimit,
  mappingHasMaxResultsArgument,
  MCP_SEARCH_RESULT_LIMIT_HEAL_TARGETS,
  presetDeclaredResultLimitTargets,
} from "@/components/ai/skills/mcpSearchMappingHeal";
import type { WebEvidenceProviderSummary } from "@/lib/ipc";

function providerSummary(
  overrides: Partial<WebEvidenceProviderSummary> &
    Pick<WebEvidenceProviderSummary, "id" | "name" | "transportConfigJson">,
): WebEvidenceProviderSummary {
  return {
    providerKind: "mcp",
    enabled: true,
    transportKind: "https",
    credentialRefsJson: "{}",
    searchMapping: null,
    fetchMapping: null,
    mappingStatus: "partial",
    diagnosticStatus: "ready",
    isNative: false,
    editable: true,
    hasSearchMapping: true,
    hasFetchMapping: false,
    ...overrides,
  };
}

describe("MCP search mapping heal targets", () => {
  it("keeps shared manifest aligned with preset-declared maxResultsArg entries", () => {
    const fromPresets = presetDeclaredResultLimitTargets().map((target) => ({
      presetId: target.presetId,
      maxResultsArg: target.maxResultsArg,
    }));
    const fromManifest = MCP_SEARCH_RESULT_LIMIT_HEAL_TARGETS.map((target) => ({
      presetId: target.presetId,
      maxResultsArg: target.maxResultsArg,
    }));
    expect(fromManifest).toEqual(fromPresets);
  });
});

describe("AnySearch search mapping heal", () => {
  const anySearch = providerSummary({
    id: "anysearch",
    name: "AnySearch",
    transportConfigJson: JSON.stringify({
      preset_id: "anysearch",
      url: "https://api.anysearch.com/mcp",
    }),
  });

  it("adds maxResultsArg when saving a legacy search mapping", () => {
    const legacy = JSON.stringify({ tool: "search", queryArg: "query" });
    const healed = ensureProviderSearchMappingResultLimit(anySearch, legacy);
    expect(healed).toContain('"maxResultsArg":"max_results"');
    expect(mappingHasMaxResultsArgument(healed)).toBe(true);
    expect(
      needsSearchResultLimitUpdate({
        ...anySearch,
        searchMapping: healed,
      }),
    ).toBe(false);
  });

  it("marks legacy persisted mappings as needing update until healed", () => {
    expect(
      needsSearchResultLimitUpdate({
        ...anySearch,
        searchMapping: JSON.stringify({ tool: "search" }),
      }),
    ).toBe(true);
  });
});

describe("Firecrawl search mapping heal", () => {
  const firecrawl = providerSummary({
    id: "firecrawl",
    name: "Firecrawl",
    transportConfigJson: JSON.stringify({
      preset_id: "firecrawl",
      url: "https://mcp.firecrawl.dev/v2/mcp",
    }),
  });

  it("adds limit when saving a legacy firecrawl_search mapping", () => {
    const legacy = JSON.stringify({
      tool: "firecrawl_search",
      queryArg: "query",
    });
    const healed = ensureProviderSearchMappingResultLimit(firecrawl, legacy);
    expect(healed).toContain('"maxResultsArg":"limit"');
    expect(
      needsSearchResultLimitUpdate({
        ...firecrawl,
        searchMapping: healed,
      }),
    ).toBe(false);
  });

  it("does not heal providers outside the manifest", () => {
    const jina = providerSummary({
      id: "jina",
      name: "Jina Reader",
      transportConfigJson: JSON.stringify({
        url: "https://mcp.jina.ai/v1",
      }),
    });
    const legacy = JSON.stringify({ tool: "search_web", queryArg: "query" });
    expect(ensureProviderSearchMappingResultLimit(jina, legacy)).toBe(legacy);
  });
});

describe("MCP credential state text", () => {
  it("distinguishes bound key, anonymous optional, and pending update", () => {
    const optionalRow = {
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
