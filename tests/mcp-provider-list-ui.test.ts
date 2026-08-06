import { describe, expect, it } from "vitest";

import { orderMcpProvidersForSearchRoute } from "@/components/ai/skills/mcpProviderListUi";

interface Provider {
  id: string;
  enabled: boolean;
  hasSearchMapping: boolean;
}

const providers: Provider[] = [
  { id: "disabled", enabled: false, hasSearchMapping: true },
  { id: "tavily", enabled: true, hasSearchMapping: true },
  { id: "read-only", enabled: true, hasSearchMapping: false },
  { id: "anysearch", enabled: true, hasSearchMapping: true },
  { id: "jina", enabled: true, hasSearchMapping: true },
  { id: "fourth", enabled: true, hasSearchMapping: true },
];

describe("MCP 联网搜索列表投影", () => {
  it("将有效候选置顶并为前三项分配主备角色", () => {
    const items = orderMcpProvidersForSearchRoute(providers, [
      "anysearch",
      "tavily",
    ]);

    expect(items.map(({ provider }) => provider.id)).toEqual([
      "anysearch",
      "tavily",
      "jina",
      "disabled",
      "read-only",
      "fourth",
    ]);
    expect(items.map(({ searchRouteRole }) => searchRouteRole)).toEqual([
      "primary",
      "fallback_1",
      "fallback_2",
      undefined,
      undefined,
      undefined,
    ]);
  });

  it("忽略失效路由项、去重并保留非候选的原始相对顺序", () => {
    const items = orderMcpProvidersForSearchRoute(providers, [
      "disabled",
      "anysearch",
      "anysearch",
      "missing",
    ]);

    expect(items.map(({ provider }) => provider.id)).toEqual([
      "anysearch",
      "tavily",
      "jina",
      "disabled",
      "read-only",
      "fourth",
    ]);
  });
});
