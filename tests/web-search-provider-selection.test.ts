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

  it("auto-picks the first MCP search provider when none is explicitly selected", () => {
    const state = getWebSearchAvailability(
      [provider("anysearch", "AnySearch"), provider("brave", "Brave Search")],
      null,
    );

    expect(state.canEnable).toBe(true);
    expect(state.reason).toBe("ready");
    expect(state.effectiveProvider?.id).toBe("anysearch");
    expect(webSearchStatusDetail(true, state)).toBe("已开启 · AnySearch");
  });

  it("falls back to auto-pick when the selected provider is unavailable", () => {
    const state = getWebSearchAvailability(
      [provider("anysearch", "AnySearch"), provider("brave", "Brave Search")],
      "missing",
    );

    expect(state.canEnable).toBe(true);
    expect(state.reason).toBe("ready");
    expect(state.effectiveProvider?.id).toBe("anysearch");
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
