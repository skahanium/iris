import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

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
    expect(center).not.toContain("MinimaxSearchSection");
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
    expect(center).not.toContain("MinimaxSearchSection");
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
    expect(center).toContain("onProvidersChanged={refreshWebProviderSummary}");
    expect(center).not.toContain('data-testid="native-provider-card-minimax"');
    expect(center).not.toContain(
      'data-testid="native-provider-card-duckduckgo"',
    );
    expect(center).not.toContain("MinimaxSearchSection");

    expect(mcpPanel).toContain('data-testid="mcp-provider-panel"');
    expect(mcpPanel).toContain("webEvidenceProvidersList");
    expect(mcpPanel).toContain("webEvidenceProviderUpsert");
    expect(mcpPanel).toContain("webEvidenceProviderDiagnostics");
    expect(mcpPanel).not.toContain("MiniMax 和 DuckDuckGo");
    expect(mcpPanel).toContain("DuckDuckGo");
    expect(mcpPanel).toContain("作为内置原生托底");
    expect(mcpPanel).toContain("不参与联网证据调度");
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
      "测试连接",
      "保存 MCP 提供方",
      "HTTPS 服务",
      "本地命令 (stdio)",
      "请求头",
      "环境变量",
      "先保存提供方，再测试连接或查看诊断。",
    ]) {
      expect(mcpCard).toContain(label);
    }

    expect(mcpPanel).toContain("persisted={false}");

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

  it("shows MCP web providers as the prioritized search backend in management overview", () => {
    const center = read("src/components/settings/ManagementCenterPanel.tsx");
    const mcpPanel = read("src/components/ai/skills/McpProfilesPanel.tsx");

    expect(center).toContain("webEvidenceProvidersList");
    expect(center).toContain("webEvidenceProviderDiagnostics");
    expect(center).toContain("MCP：");
    expect(center).toContain("原生兜底");
    expect(center).toContain("refreshWebProviderSummary");
    expect(center).toContain("onProvidersChanged={refreshWebProviderSummary}");
    expect(center).not.toMatch(/effectiveBackend === "minimax"/);
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

  it("waits for restored notes to open before closing recycle views", () => {
    const recycle = read("src/components/file/RecycleBinSheet.tsx");
    const restoreIndex = recycle.indexOf("await onRestored(path)");
    const closeIndex = recycle.indexOf("onClose();", restoreIndex);

    expect(restoreIndex).toBeGreaterThanOrEqual(0);
    expect(closeIndex).toBeGreaterThan(restoreIndex);
  });
});
