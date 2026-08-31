import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { readFileSync } from "node:fs";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { McpProfilesPanel } from "@/components/ai/skills/McpProfilesPanel";
import type {
  McpCapabilityBindingSummary,
  McpReadOnlyToolCandidate,
  WebEvidenceProviderSummary,
} from "@/lib/ipc";

const ipcMocks = vi.hoisted(() => ({
  credentialDelete: vi.fn(),
  credentialSet: vi.fn(),
  credentialStatus: vi.fn(),
  mcpCapabilityBindingDelete: vi.fn(),
  mcpCapabilityBindingsList: vi.fn(),
  mcpCapabilityBindingUpsert: vi.fn(),
  mcpReadOnlyToolsDiscover: vi.fn(),
  webEvidenceProviderDelete: vi.fn(),
  webEvidenceProviderDiagnostics: vi.fn(),
  webEvidenceProvidersList: vi.fn(),
  webEvidenceProviderToggle: vi.fn(),
  webEvidenceProviderUpsert: vi.fn(),
  webSearchRouteGet: vi.fn(),
  webSearchRouteSet: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => true,
}));

vi.mock("@/lib/ipc", () => ipcMocks);

const provider: WebEvidenceProviderSummary = {
  id: "readonly-provider",
  name: "ReadonlyProvider",
  providerKind: "mcp",
  enabled: true,
  transportKind: "https",
  transportConfigJson: JSON.stringify({
    url: "https://readonly.example.com/mcp",
  }),
  credentialRefsJson: "{}",
  searchMapping: "search",
  fetchMapping: "fetch",
  mappingStatus: "complete",
  diagnosticStatus: "ready",
  isNative: false,
  editable: true,
  hasSearchMapping: true,
  hasFetchMapping: true,
};

const readOnlyTool: McpReadOnlyToolCandidate = {
  providerDisplayName: provider.name,
  providerConfigHash: "provider-config-sha256",
  bindingConfigHash: "binding-config-sha256",
  name: "get_records",
  inputSchema: {
    type: "object",
    properties: { query: { type: "string" } },
  },
  riskClass: "read_only",
  readOnly: true,
};

function bindingSummary(
  overrides: Partial<McpCapabilityBindingSummary> = {},
): McpCapabilityBindingSummary {
  return {
    id: "binding-1",
    providerId: provider.id,
    exposedName: "external_read_records",
    mcpToolName: readOnlyTool.name,
    inputSchema: readOnlyTool.inputSchema,
    argumentMapping: {},
    outputPolicy: {
      mode: "text_or_json",
      maxModelChars: 8000,
      maxEvidenceChars: 2000,
    },
    providerConfigHash: "provider-config-sha256",
    bindingConfigHash: "binding-config-sha256",
    providerEnabled: true,
    configMatches: true,
    userTrusted: true,
    ...overrides,
  };
}

async function openDiscoveredReadOnlyTool() {
  render(<McpProfilesPanel open />);
  await screen.findByTestId("mcp-provider-panel");
  fireEvent.click(screen.getByTestId("mcp-provider-card"));
  await screen.findByTestId("mcp-provider-detail");
  fireEvent.click(screen.getByRole("button", { name: "发现只读工具" }));
  await screen.findByTestId(`mcp-external-read-tool-${readOnlyTool.name}`);
}

describe("McpProfilesPanel 外部只读 binding", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.spyOn(window, "confirm").mockReturnValue(true);
    ipcMocks.credentialDelete.mockResolvedValue(undefined);
    ipcMocks.credentialSet.mockResolvedValue(undefined);
    ipcMocks.credentialStatus.mockResolvedValue({
      service: "iris.mcp.readonly",
      state: "available",
      configured: true,
      checkedAt: new Date().toISOString(),
    });
    ipcMocks.mcpCapabilityBindingDelete.mockResolvedValue(undefined);
    ipcMocks.mcpCapabilityBindingsList.mockResolvedValue([]);
    ipcMocks.mcpCapabilityBindingUpsert.mockResolvedValue(bindingSummary());
    ipcMocks.mcpReadOnlyToolsDiscover.mockResolvedValue({
      providerId: provider.id,
      tools: [readOnlyTool],
      rejectedCount: 0,
    });
    ipcMocks.webEvidenceProviderDelete.mockResolvedValue(undefined);
    ipcMocks.webEvidenceProviderDiagnostics.mockResolvedValue({
      providerId: provider.id,
      status: "ready",
      failures: [],
      checks: [],
      canUseForSearch: true,
      canUseForFetch: true,
    });
    ipcMocks.webEvidenceProvidersList.mockResolvedValue([provider]);
    ipcMocks.webEvidenceProviderToggle.mockResolvedValue(undefined);
    ipcMocks.webEvidenceProviderUpsert.mockResolvedValue(undefined);
    ipcMocks.webSearchRouteGet.mockResolvedValue({
      candidateProviderIds: [provider.id],
    });
    ipcMocks.webSearchRouteSet.mockResolvedValue({
      candidateProviderIds: [provider.id],
    });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("新发现的工具只创建 generic external.read binding", async () => {
    await openDiscoveredReadOnlyTool();

    expect(
      screen.queryByTestId(`mcp-domain-operation-${readOnlyTool.name}`),
    ).toBeNull();
    expect(screen.queryByText("保存当前事实映射")).toBeNull();

    fireEvent.click(
      screen.getByTestId(`mcp-external-bind-${readOnlyTool.name}`),
    );
    await waitFor(() => {
      expect(ipcMocks.mcpCapabilityBindingUpsert).toHaveBeenCalledWith({
        providerId: provider.id,
        mcpToolName: readOnlyTool.name,
        inputSchema: readOnlyTool.inputSchema,
        argumentMapping: {},
        riskClass: "read_only",
        readOnly: true,
        userTrusted: true,
        attestedBindingConfigHash: readOnlyTool.bindingConfigHash,
      });
    });
    expect(screen.getByText(/只读工具绑定已保存/)).toBeTruthy();
  });

  it("旧领域 binding 仅显示为可删除的兼容记录", async () => {
    ipcMocks.mcpCapabilityBindingsList.mockResolvedValue([
      bindingSummary({
        id: "legacy-domain-binding",
        domainOperation: "weather.current",
        outputMapping: {
          recordsPath: "$.records",
          fields: { location: "$.location" },
        },
      }),
    ]);

    render(<McpProfilesPanel open />);
    await screen.findByTestId("mcp-provider-panel");
    fireEvent.click(screen.getByTestId("mcp-provider-card"));

    expect(await screen.findByText(/已退役的旧领域映射/)).toBeTruthy();
    expect(screen.queryByText(/已配置当前事实/)).toBeNull();
    expect(screen.getByRole("button", { name: "删除绑定" })).toBeTruthy();
  });

  it("Composer 只加载 generic external.read binding", () => {
    const panel = readFileSync(
      "src/components/ai/UnifiedAssistantPanel.impl.tsx",
      "utf8",
    );

    expect(panel).toContain("!binding.domainOperation");
    expect(panel).toContain("!binding.outputMapping");
  });
});
