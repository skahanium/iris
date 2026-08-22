import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
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
  id: "weather-provider",
  name: "WeatherProvider",
  providerKind: "mcp",
  enabled: true,
  transportKind: "https",
  transportConfigJson: JSON.stringify({
    url: "https://weather.example.com/mcp",
  }),
  credentialRefsJson: JSON.stringify({
    headers: {
      Authorization: {
        credential: "credential://iris.mcp.weather",
        scheme: "bearer",
      },
    },
  }),
  searchMapping: "search",
  fetchMapping: "fetch",
  mappingStatus: "complete",
  diagnosticStatus: "ready",
  isNative: false,
  editable: true,
  hasSearchMapping: true,
  hasFetchMapping: true,
};

const weatherTool: McpReadOnlyToolCandidate = {
  providerDisplayName: provider.name,
  providerConfigHash: "provider-config-sha256",
  bindingConfigHash: "binding-config-sha256",
  name: "get_current_weather",
  inputSchema: {
    type: "object",
    properties: {
      location: { type: "string" },
      temperature: { type: "number" },
      observedAt: { type: "string" },
      sourceUrl: { type: "string" },
    },
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
    exposedName: "domain_read_weather",
    mcpToolName: weatherTool.name,
    inputSchema: weatherTool.inputSchema,
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

async function openDiscoveredWeatherTool(toolName = weatherTool.name) {
  render(<McpProfilesPanel open />);
  await screen.findByTestId("mcp-provider-panel");
  fireEvent.click(screen.getByTestId("mcp-provider-card"));
  await screen.findByTestId("mcp-provider-detail");
  fireEvent.click(screen.getByRole("button", { name: "发现只读工具" }));
  await screen.findByTestId(`mcp-domain-mapping-tool-${toolName}`);
}

function fillWeatherMapping() {
  fireEvent.change(
    screen.getByTestId(`mcp-domain-operation-${weatherTool.name}`),
    { target: { value: "weather.current" } },
  );
  fireEvent.change(
    screen.getByTestId(`mcp-domain-records-path-${weatherTool.name}`),
    { target: { value: "$.records" } },
  );
  for (const [field, path] of [
    ["location", "$.location"],
    ["temperature", "$.temperature"],
    ["observedAt", "$.observedAt"],
    ["sourceUrl", "$.sourceUrl"],
  ]) {
    fireEvent.change(
      screen.getByTestId(`mcp-domain-field-${weatherTool.name}-${field}`),
      { target: { value: path } },
    );
  }
}

async function clickSaveDomainMapping() {
  fireEvent.click(screen.getByTestId(`mcp-domain-save-${weatherTool.name}`));
  await waitFor(() => {
    expect(ipcMocks.mcpCapabilityBindingUpsert).toHaveBeenCalled();
  });
}

describe("McpProfilesPanel 当前事实低配置映射", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    ipcMocks.credentialDelete.mockResolvedValue(undefined);
    ipcMocks.credentialSet.mockResolvedValue(undefined);
    ipcMocks.credentialStatus.mockResolvedValue({
      service: "iris.mcp.weather",
      state: "available",
      configured: true,
      checkedAt: new Date().toISOString(),
    });
    ipcMocks.mcpCapabilityBindingDelete.mockResolvedValue(undefined);
    ipcMocks.mcpCapabilityBindingsList.mockResolvedValue([]);
    ipcMocks.mcpCapabilityBindingUpsert.mockResolvedValue(
      bindingSummary({
        domainOperation: "weather.current",
        outputMapping: {
          recordsPath: "$.records",
          fields: {
            location: "$.location",
            temperature: "$.temperature",
            observedAt: "$.observedAt",
            sourceUrl: "$.sourceUrl",
          },
        },
      }),
    );
    ipcMocks.mcpReadOnlyToolsDiscover.mockResolvedValue({
      providerId: provider.id,
      tools: [weatherTool],
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

  it("保存只读天气工具的当前事实映射并发送规范化的 domainOperation/outputMapping", async () => {
    await openDiscoveredWeatherTool();
    fillWeatherMapping();
    await clickSaveDomainMapping();

    expect(ipcMocks.mcpCapabilityBindingUpsert).toHaveBeenCalledWith({
      providerId: provider.id,
      mcpToolName: weatherTool.name,
      inputSchema: weatherTool.inputSchema,
      argumentMapping: {},
      domainOperation: "weather.current",
      outputMapping: {
        recordsPath: "$.records",
        fields: {
          location: "$.location",
          temperature: "$.temperature",
          observedAt: "$.observedAt",
          sourceUrl: "$.sourceUrl",
        },
      },
      riskClass: "read_only",
      readOnly: true,
      userTrusted: true,
      attestedBindingConfigHash: weatherTool.bindingConfigHash,
    });
    expect(screen.getByText(/已保存当前事实映射/)).toBeTruthy();
  });

  it("高级区只显示只读 schema 与配置哈希，不暴露 transport/credential", async () => {
    await openDiscoveredWeatherTool();
    fireEvent.click(
      screen.getByTestId(`mcp-domain-advanced-${weatherTool.name}`),
    );
    await screen.findByTestId(`mcp-domain-schema-${weatherTool.name}`);
    expect(screen.getByText(/get_current_weather/)).toBeTruthy();
    expect(screen.getByText(/provider-config-sha256/)).toBeTruthy();
    expect(screen.getByText(/binding-config-sha256/)).toBeTruthy();
    expect(
      screen.getByTestId(`mcp-domain-schema-${weatherTool.name}`).textContent,
    ).toContain('"type": "object"');
    expect(
      screen.getByTestId(`mcp-domain-advanced-${weatherTool.name}`).textContent,
    ).not.toContain("https://weather.example.com");
    expect(
      screen.getByTestId(`mcp-domain-advanced-${weatherTool.name}`).textContent,
    ).not.toContain("credential://");
  });

  it("拒绝保存写操作工具且错误不显示 transport config 或 credential ref", async () => {
    const writeTool = {
      ...weatherTool,
      name: "write_weather",
      readOnly: false,
      riskClass: "write",
    } as unknown as McpReadOnlyToolCandidate;
    ipcMocks.mcpReadOnlyToolsDiscover.mockResolvedValue({
      providerId: provider.id,
      tools: [writeTool],
      rejectedCount: 0,
    });

    await openDiscoveredWeatherTool(writeTool.name);
    fireEvent.change(
      screen.getByTestId(`mcp-domain-operation-${writeTool.name}`),
      { target: { value: "weather.current" } },
    );
    fireEvent.click(screen.getByTestId(`mcp-domain-save-${writeTool.name}`));

    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toContain("只读");
    });
    expect(ipcMocks.mcpCapabilityBindingUpsert).not.toHaveBeenCalled();
    const alert = screen.getByRole("alert").textContent ?? "";
    expect(alert).not.toContain("transport");
    expect(alert).not.toContain("credential");
    expect(alert).not.toContain("https://weather.example.com");
    expect(alert).not.toContain("credential://");
  });

  it("拒绝缺少 source/time 映射的当前事实保存", async () => {
    await openDiscoveredWeatherTool();
    fireEvent.change(
      screen.getByTestId(`mcp-domain-operation-${weatherTool.name}`),
      { target: { value: "weather.current" } },
    );
    fireEvent.change(
      screen.getByTestId(`mcp-domain-records-path-${weatherTool.name}`),
      { target: { value: "$.records" } },
    );
    fireEvent.change(
      screen.getByTestId(`mcp-domain-field-${weatherTool.name}-location`),
      { target: { value: "$.location" } },
    );
    fireEvent.change(
      screen.getByTestId(`mcp-domain-field-${weatherTool.name}-temperature`),
      { target: { value: "$.temperature" } },
    );
    fireEvent.click(screen.getByTestId(`mcp-domain-save-${weatherTool.name}`));

    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toContain("观测时间");
    });
    expect(screen.getByRole("alert").textContent).toContain("来源 URL");
    expect(ipcMocks.mcpCapabilityBindingUpsert).not.toHaveBeenCalled();
    const alert = screen.getByRole("alert").textContent ?? "";
    expect(alert).not.toContain("transport");
    expect(alert).not.toContain("credential");
    expect(alert).not.toContain("https://weather.example.com");
    expect(alert).not.toContain("credential://");
  });

  it("拒绝非法 JSON path 且错误不显示 transport/credential", async () => {
    await openDiscoveredWeatherTool();
    fillWeatherMapping();
    fireEvent.change(
      screen.getByTestId(`mcp-domain-field-${weatherTool.name}-sourceUrl`),
      { target: { value: "sourceUrl" } },
    );
    fireEvent.click(screen.getByTestId(`mcp-domain-save-${weatherTool.name}`));

    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toContain("JSON path");
    });
    expect(ipcMocks.mcpCapabilityBindingUpsert).not.toHaveBeenCalled();
    const alert = screen.getByRole("alert").textContent ?? "";
    expect(alert).not.toContain("transport");
    expect(alert).not.toContain("credential");
    expect(alert).not.toContain("https://weather.example.com");
    expect(alert).not.toContain("credential://");
  });

  it("同一 provider 重复 operation 不能保存", async () => {
    ipcMocks.mcpCapabilityBindingsList.mockResolvedValue([
      bindingSummary({
        id: "existing-weather",
        mcpToolName: "another_weather_tool",
        domainOperation: "weather.current",
        outputMapping: {
          recordsPath: "$.records",
          fields: { location: "$.location" },
        },
      }),
    ]);

    await openDiscoveredWeatherTool();
    fillWeatherMapping();
    fireEvent.click(screen.getByTestId(`mcp-domain-save-${weatherTool.name}`));

    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toContain(
        "weather.current",
      );
    });
    expect(ipcMocks.mcpCapabilityBindingUpsert).not.toHaveBeenCalled();
    const alert = screen.getByRole("alert").textContent ?? "";
    expect(alert).not.toContain("transport");
    expect(alert).not.toContain("credential");
    expect(alert).not.toContain("https://weather.example.com");
    expect(alert).not.toContain("credential://");
  });
});
