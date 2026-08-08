import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(relativePath: string): string {
  return readFileSync(relativePath, "utf8");
}

describe("管理中心供应商子页", () => {
  it("LLM 与 MCP 面板支持第三级详情与 overlay providerId", () => {
    const center = read("src/components/settings/ManagementCenterPanel.tsx");
    const primitives = read(
      "src/components/settings/managementCenterPrimitives.tsx",
    );
    const llm = read("src/components/settings/LlmRoutingSection.tsx");
    const llmDetail = read("src/components/settings/LlmProviderDetail.tsx");
    const mcpPanel = read("src/components/ai/skills/McpProfilesPanel.tsx");
    const mcpDetail = read("src/components/ai/skills/McpProviderDetail.tsx");

    expect(center).toContain("managementCenterProviderId");
    expect(center).toContain("onManagementCenterProviderIdChange");
    expect(center).toContain("onProviderChromeChange");
    expect(primitives).toContain("data-management-provider-detail");
    expect(llm).toContain("LlmProviderDetail");
    expect(llm).toContain("LlmProviderListCard");
    expect(llm).not.toContain(
      '<h3 className="text-sm font-medium">模型与供应商</h3>',
    );
    expect(llmDetail).not.toContain("llm-provider-detail-back");
    expect(mcpPanel).toContain("selectedProviderId");
    expect(mcpPanel).toContain("onProviderChromeChange");
    expect(mcpDetail).toContain("McpProviderDetail");
    expect(mcpDetail).toContain('surface="detail"');
  });

  it("MCP 详情默认提供高级设置折叠入口", () => {
    const card = read("src/components/ai/skills/McpProfileCard.tsx");
    expect(card).toContain("mcp-provider-advanced-trigger");
    expect(card).toContain("mcp-provider-basic-key");
    expect(card).toContain('surface === "list"');
    expect(card).toContain("ChevronRight");
    expect(card).toContain("mcpListDotTone");
    expect(card).not.toContain("mcp-provider-detail-back");
  });
});
