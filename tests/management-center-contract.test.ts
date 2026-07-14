import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

const removedDdg = ["duck", "duck", "go"].join("");
const removedVendor = ["mini", "max"].join("");

describe("management center contract", () => {
  it("uses top tabs for four management sections without the old sidebar", () => {
    const center = read("src/components/settings/ManagementCenterPanel.tsx");
    const overlays = read("src/components/layout/AppOverlays.tsx");
    const app = read("src/App.impl.tsx");

    expect(center).toContain('data-testid="management-center"');
    expect(center).toContain('data-testid="management-center-tabs"');
    expect(center).toContain("grid-cols-4");
    expect(center).toContain("w-full");
    expect(center).not.toContain('data-testid="management-center-nav"');
    for (const id of [
      'id: "overview"',
      'id: "notes"',
      'id: "knowledge"',
      'id: "ai"',
    ]) {
      expect(center).toContain(id);
    }
    expect(center).not.toContain('id: "workspace"');
    expect(center).not.toContain('id: "security"');
    expect(center).not.toContain('{ id: "about"');
    expect(center).toContain("ManagementCenterSection");
    expect(center).toContain("LlmRoutingSection");
    expect(center.toLowerCase()).not.toContain(`${removedVendor}searchsection`);
    expect(center).toContain("PersonaSettingsBody");
    expect(center).toContain("SkillsPanelBody");
    expect(center).toContain("AiRulesPanel");
    expect(center).not.toContain('data-testid="ai-system-center-nav"');
    expect(overlays).toContain("ManagementCenterPanel");
    expect(overlays).not.toContain("SettingsPanel");
    expect(overlays).not.toContain("AiSystemCenterPanel");
    expect(app).toContain('openManagementCenter("ai")');
  });

  it("keeps knowledge management inline and moves graph to the status bar", () => {
    const center = read("src/components/settings/ManagementCenterPanel.tsx");
    const overlays = read("src/components/layout/AppOverlays.tsx");
    const statusBar = read("src/components/layout/StatusBar.tsx");
    const statusSlot = read("src/components/layout/AppStatusBarSlot.tsx");

    for (const prop of [
      "onOpenKnowledgeRelations",
      "onOpenVersion",
      "onRescanVault",
    ]) {
      expect(center, prop).toContain(prop);
      expect(overlays, prop).toContain(prop);
    }

    expect(center).toContain("VaultNavigatorBody");
    expect(center).toContain("RecycleBinBody");
    expect(center).not.toContain("renderTaskDetail");
    expect(center).not.toContain("openDetail");
    expect(statusBar).toContain('data-testid="status-bar-graph-button"');
    expect(statusSlot).toContain("onOpenGraph");
    expect(center).not.toContain("openClassified");
    expect(center).not.toContain("onOpenClassifiedPanel");
  });

  it("exposes automatic version tracking as real settings in the notes area", () => {
    const center = read("src/components/settings/ManagementCenterPanel.tsx");
    const app = read("src/App.impl.tsx");
    const lifecycle = read("src/hooks/useAppPersistenceLifecycle.ts");
    const idle = read("src/hooks/useVersionIdle.ts");

    expect(center).toContain("autoVersionEnabled");
    expect(center).toContain("autoVersionIdleMinutes");
    expect(app).toContain("useAutoVersionSettings");
    expect(lifecycle).toContain("autoVersionEnabled");
    expect(lifecycle).toContain("autoVersionIdleMinutes");
    expect(idle).toContain("enabled");
    expect(idle).toContain("idleMs");
  });

  it("allows AI-only drilldown while keeping management sections scrollable", () => {
    const center = read("src/components/settings/ManagementCenterPanel.tsx");

    expect(center).toContain('data-testid="management-section-ai"');
    expect(center).toContain('data-testid="management-ai-detail"');
    expect(center).toContain('data-testid="management-detail-back"');
    expect(center).toContain("overflow-y-auto");
    expect(center).toContain("LlmRoutingSection");
    expect(center.toLowerCase()).not.toContain(`${removedVendor}searchsection`);
    expect(center).toContain("PersonaSettingsBody");
    expect(center).toContain("SkillsPanelBody");
    expect(center).toContain("AiRulesPanel");
    expect(center).toContain("renderAiDetail");
    expect(center).toContain("openAiDetail");
  });

  it("renders native and MCP web evidence provider management surfaces", () => {
    const center = read("src/components/settings/ManagementCenterPanel.tsx");
    const mcpPanel = read("src/components/ai/skills/McpProfilesPanel.tsx");
    const mcpCard = read("src/components/ai/skills/McpProfileCard.tsx");
    const mcpPresets = read("src/components/ai/skills/mcpProviderPresets.ts");

    expect(center).toContain("McpProfilesPanel");
    expect(center).toContain("<McpProfilesPanel");
    expect(center).toContain(
      "onProvidersChanged={onRefreshWebSearchProviders}",
    );
    expect(center.toLowerCase()).not.toContain(
      `data-testid="native-provider-card-${removedVendor}"`,
    );
    expect(center).not.toContain(
      `data-testid="native-provider-card-${removedDdg}"`,
    );
    expect(center.toLowerCase()).not.toContain(`${removedVendor}searchsection`);

    expect(mcpPanel).toContain('data-testid="mcp-provider-panel"');
    expect(mcpPanel).toContain("webEvidenceProvidersList");
    expect(mcpPanel).toContain("webEvidenceProviderUpsert");
    expect(mcpPanel).toContain("webEvidenceProviderDiagnostics");
    expect(mcpPanel.toLowerCase()).not.toContain(removedDdg);
    expect(mcpPanel).not.toContain("原生托底");
    expect(mcpPanel).toContain("当前选择");
    expect(mcpPanel).toContain("McpProfileCard");
    expect(mcpPanel).not.toContain("MCP_PROVIDER_PRESETS.map");
    expect(mcpPanel).not.toContain("setDraft(createDraftSummary(preset))");
    expect(mcpCard).toContain("MCP_PROVIDER_PRESETS.map");
    expect(mcpCard).not.toContain("MCP_PROVIDER_PRESETS.slice(0, 6)");
    expect(mcpCard).not.toContain(
      'variant={presetId === preset.id ? "secondary" : "outline"}',
    );
    expect(mcpPanel).toContain(
      "点击添加 MCP 提供方后，可选择预设或自定义服务。",
    );

    for (const label of ["MCP 联网证据提供方", "添加 MCP 提供方"]) {
      expect(mcpPanel).toContain(label);
    }

    for (const label of [
      "提供方名称",
      "连接方式",
      "HTTPS 服务地址",
      "允许连接本机开发服务",
      "stdio 启动命令",
      "启动参数",
      "凭据引用",
      "搜索工具映射",
      "网页读取工具映射",
      "实时诊断",
      "保存 MCP 提供方",
      "HTTPS 服务",
      "本地命令 (stdio)",
      "请求头",
      "环境变量",
      "先保存提供方，再执行实时诊断。",
    ]) {
      expect(mcpCard).toContain(label);
    }

    // The preset entry is a single dropdown; its "快速预设" label and the
    // "自定义 MCP 服务" option must each render inside the card.
    expect(mcpCard).toContain("快速预设");
    expect(mcpCard).toContain("自定义 MCP 服务");

    expect(mcpPanel).toContain("persisted={false}");
    expect(mcpCard).not.toContain("测试连接");
    expect(mcpCard).not.toContain("测试后");

    for (const leakedLabel of [
      "MCP web evidence providers",
      "Add MCP provider",
      "Provider name",
      "Search mapping",
      "Fetch mapping",
      "Test connection",
      "Save MCP provider",
      "HTTPS MCP",
      "stdio MCP",
      "Header/Env 名",
      ">Header</SelectItem>",
      ">Env</SelectItem>",
    ]) {
      expect(mcpPanel + mcpCard + mcpPresets).not.toContain(leakedLabel);
    }
    expect(mcpCard).not.toMatch(/>\s*transportKind:/);
    expect(mcpCard).not.toMatch(/>\s*mappingStatus:/);
    expect(mcpCard).not.toMatch(/>\s*diagnosticStatus:/);
    expect(mcpCard).not.toMatch(/>\s*transportConfigJson\s*</);
    expect(mcpCard).not.toMatch(/>\s*credentialRefsJson\s*</);
    expect(mcpPanel).not.toContain("return null");
    expect(mcpCard).not.toContain("return null");

    for (const preset of [
      "AnySearch",
      "Brave Search",
      "Jina Reader",
      "Firecrawl",
      "SearXNG",
      "Tavily",
      "https://api.anysearch.com/mcp",
      "search",
      "extract",
      "tavily-search",
      "tavily-extract",
      "firecrawl_search",
      "firecrawl_scrape",
      "brave_web_search",
      "search_web",
      "read_url",
      "searxng_web_search",
      "web_url_read",
      "系统凭据",
      "API Key",
      "Authorization",
      "Bearer",
    ]) {
      expect(mcpPanel + mcpCard + mcpPresets).toContain(preset);
    }
  });

  it("requires a selected MCP web provider in management overview", () => {
    const center = read("src/components/settings/ManagementCenterPanel.tsx");
    const mcpPanel = read("src/components/ai/skills/McpProfilesPanel.tsx");

    expect(center).toContain("webSearchAvailability");
    expect(center).toContain("webSearchProviderId");
    expect(center).toContain("onWebSearchProviderChange");
    expect(center).toContain(
      "onProvidersChanged={onRefreshWebSearchProviders}",
    );
    expect(center).not.toContain("webEvidenceProvidersList");
    expect(center).not.toContain("原生托底");
    expect(center).not.toContain("refreshWebProviderSummary");
    expect(center.toLowerCase()).not.toContain("effectivebackend");
    expect(center.toLowerCase()).not.toContain(removedVendor);
    expect(mcpPanel).toContain("onProvidersChanged");
    expect(mcpPanel).toContain("onProvidersChanged?.()");
  });

  it("anchors management switches so the thumb stays inside the track", () => {
    const center = read("src/components/settings/ManagementCenterPanel.tsx");

    expect(center).toContain('role="switch"');
    expect(center).toContain("left-1 top-1");
    expect(center).toContain("translate-x-5");
    expect(center).not.toContain("translate-x-6");
  });

  it("presents system and about information in overview without classified or fake button affordances", () => {
    const center = read("src/components/settings/ManagementCenterPanel.tsx");

    expect(center).toContain("GNU Affero General Public License v3.0");
    expect(center).not.toContain("openClassified");
    expect(center).not.toContain("LockKeyhole");
    expect(center).not.toContain("secret.read_plaintext");
  });

  it("prepares notes from the embedded file tree before opening", () => {
    const center = read("src/components/settings/ManagementCenterPanel.tsx");
    const overlays = read("src/components/layout/AppOverlays.tsx");

    expect(center).toContain("onPrepareNote");
    expect(center).toContain("onPrepare={onPrepareNote}");
    expect(overlays).toContain('onPrepareNote?.(file, "management")');
  });

  it("passes file lifecycle callbacks into the embedded management file tree", () => {
    const center = read("src/components/settings/ManagementCenterPanel.tsx");
    const overlays = read("src/components/layout/AppOverlays.tsx");

    for (const prop of [
      "onBeforeFilePathChange",
      "onFilePathChanged",
      "onBeforeFileDelete",
      "onFileDeleted",
      "onIndexChange",
    ]) {
      expect(center).toContain(prop);
    }
    expect(center).toContain("onBeforeFilePathChange={onBeforeFilePathChange}");
    expect(center).toContain("onFilePathChanged={onFilePathChanged}");
    expect(center).toContain("onBeforeFileDelete={onBeforeFileDelete}");
    expect(center).toContain("onFileDeleted={onFileDeleted}");
    expect(center).toContain("onIndexChange={onIndexChange}");
    expect(overlays).toContain(
      "onBeforeFilePathChange={onBeforeFilePathChange}",
    );
    expect(overlays).toContain("onFilePathChanged={onFilePathChanged}");
    expect(overlays).toContain("onBeforeFileDelete={onBeforeFileDelete}");
    expect(overlays).toContain("onFileDeleted={onFileDeleted}");
    expect(overlays).toContain("onIndexChange={bumpVaultIndex}");
  });

  it("renders MCP provider diagnostic messages that match their pass/fail status", () => {
    const diagnostics = read("src-tauri/src/commands/ai_commands.rs");
    const broker = read("src-tauri/src/ai_runtime/web_evidence_broker.rs");
    const card = read("src/components/ai/skills/McpProfileCard.tsx");

    // Backend must emit status-aware messages for the mapping checks so the
    // UI never shows "已配置搜索映射" against a failed (NULL) mapping again.
    expect(diagnostics).toContain("已配置搜索映射");
    expect(diagnostics).toContain("未配置搜索映射");
    expect(diagnostics).toContain("已配置网页读取映射");
    expect(diagnostics).toContain("未配置网页读取映射");
    expect(diagnostics).toContain("Key 已绑定");
    expect(diagnostics).toContain("可选凭据未绑定，使用匿名模式");
    expect(diagnostics).toContain("必填凭据缺失");
    expect(diagnostics).toContain("搜索调用正常，解析出");
    expect(broker).toContain("auth header present");
    expect(broker).toContain("parsed rows");
    expect(diagnostics).toContain("提供方未启用");
    expect(diagnostics).toContain("连接方式不支持 MCP 联网证据");
    expect(diagnostics).toContain("MCP 搜索结果无法归一化为联网证据");
    expect(diagnostics).toContain(
      "MCP 服务要求 OAuth 鉴权流程，当前预设不兼容",
    );

    // The diagnostic line is rendered as label：status · message; the card
    // must keep the status text and message on the same line so users can
    // cross-check them.
    expect(card).toContain(
      "{checkLabelText(check.label)}：{checkStatusText(check.status)} ·",
    );
    expect(card).toContain("实时可用性");
    expect(card).not.toContain("配置可调度性");
    expect(card).toContain('case "credential"');
    expect(card).toContain('case "searchSmokeLive"');
    expect(card).toContain('case "searchResultParseLive"');
    expect(card).toContain("onClick={() => void onDiagnostics()}");
    expect(card).not.toContain("onDiagnostics(true)");
    expect(card).not.toContain("onDiagnostics(false)");
  });

  it("waits for restored notes to open before closing recycle views", () => {
    const recycle = read("src/components/file/RecycleBinSheet.tsx");
    const restoreIndex = recycle.indexOf("await onRestored(path)");
    const closeIndex = recycle.indexOf("onClose();", restoreIndex);

    expect(restoreIndex).toBeGreaterThanOrEqual(0);
    expect(closeIndex).toBeGreaterThan(restoreIndex);
  });
});
