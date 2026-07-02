import { describe, expect, it } from "vitest";

import {
  getWebSearchAvailability,
  webSearchStatusDetail,
  type WebSearchProviderOption,
} from "../src/lib/web-search-provider-state";

const provider = (id: string, name: string): WebSearchProviderOption => ({
  id,
  name,
  providerKind: "mcp",
  enabled: true,
  hasSearchMapping: true,
});

describe("web search provider state", () => {
  it("disables web search when no MCP search provider is available", () => {
    const state = getWebSearchAvailability([], null);

    expect(state.canEnable).toBe(false);
    expect(state.reason).toBe("missing_provider");
    expect(state.detail).toContain("未配置");
  });

  it("requires an explicit choice when multiple MCP search providers are available", () => {
    const state = getWebSearchAvailability(
      [provider("anysearch", "AnySearch"), provider("brave", "Brave Search")],
      null,
    );

    expect(state.canEnable).toBe(false);
    expect(state.reason).toBe("provider_unselected");
    expect(state.detail).toContain("选择");
  });

  it("uses the selected provider as the Agent status detail without request counts", () => {
    const state = getWebSearchAvailability(
      [provider("anysearch", "AnySearch"), provider("brave", "Brave Search")],
      "brave",
    );

    expect(state.canEnable).toBe(true);
    expect(state.effectiveProvider?.id).toBe("brave");
    expect(webSearchStatusDetail(true, state)).toBe("已开启 · Brave Search");
    expect(webSearchStatusDetail(true, state)).not.toContain("MCP");
    expect(webSearchStatusDetail(true, state)).not.toContain("DDG");
    expect(webSearchStatusDetail(true, state)).not.toContain("次");
  });
});
