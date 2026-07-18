import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { readFileSync } from "node:fs";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { McpProfilesPanel } from "@/components/ai/skills/McpProfilesPanel";

const ipcMocks = vi.hoisted(() => ({
  credentialDelete: vi.fn(),
  credentialSet: vi.fn(),
  webEvidenceProviderDelete: vi.fn(),
  webEvidenceProviderDiagnostics: vi.fn(),
  webEvidenceProvidersList: vi.fn(),
  webEvidenceProviderToggle: vi.fn(),
  webEvidenceProviderUpsert: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => true,
}));

vi.mock("@/lib/ipc", () => ipcMocks);

const provider = {
  id: "anysearch",
  name: "AnySearch",
  providerKind: "mcp",
  enabled: true,
  transportKind: "https",
  transportConfigJson: JSON.stringify({
    url: "https://api.anysearch.com/mcp",
  }),
  credentialRefsJson: JSON.stringify({
    headers: {
      Authorization: {
        credential: "credential://iris.mcp.anysearch",
        scheme: "bearer",
        optional: true,
      },
    },
  }),
  searchMapping: "search",
  fetchMapping: "extract",
  mappingStatus: "complete",
  diagnosticStatus: "ready",
  isNative: false,
  editable: true,
  hasSearchMapping: true,
  hasFetchMapping: true,
};

const liveDiagnostics = {
  providerId: provider.id,
  status: "ready",
  failures: [],
  checks: [
    {
      label: "liveConnection",
      status: "pass",
      message: "MCP 服务已响应 tools/list",
    },
  ],
  canUseForSearch: true,
  canUseForFetch: true,
};

async function flush(): Promise<void> {
  await act(async () => {
    await Promise.resolve();
  });
}

function button(host: HTMLElement, text: string): HTMLButtonElement {
  const result = Array.from(host.querySelectorAll("button")).find(
    (item) => item.textContent?.trim() === text,
  );
  if (!result) throw new Error(`missing button: ${text}`);
  return result;
}

describe("McpProfilesPanel 实时诊断", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    vi.clearAllMocks();
    ipcMocks.credentialDelete.mockResolvedValue(undefined);
    ipcMocks.credentialSet.mockResolvedValue(undefined);
    ipcMocks.webEvidenceProviderDelete.mockResolvedValue(undefined);
    ipcMocks.webEvidenceProviderDiagnostics.mockResolvedValue(liveDiagnostics);
    ipcMocks.webEvidenceProvidersList.mockResolvedValue([provider]);
    ipcMocks.webEvidenceProviderToggle.mockResolvedValue(undefined);
    ipcMocks.webEvidenceProviderUpsert.mockResolvedValue(undefined);
  });

  it("将诊断 IPC 固定为指定提供方的实时检查，不再暴露静态检查开关", () => {
    const ipc = readFileSync("src/lib/ipc.ts", "utf8");
    const command = readFileSync(
      "src-tauri/src/commands/ai_commands.rs",
      "utf8",
    );

    expect(ipc).toContain("providerId: string");
    expect(ipc).not.toContain("liveCheck");
    expect(command).toContain("provider_id: String");
    expect(command).not.toContain("live_check: Option<bool>");
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    host.remove();
    vi.restoreAllMocks();
  });

  it("打开或重新打开面板时不自动诊断，且只保留一个实时诊断入口", async () => {
    await act(async () => {
      root.render(<McpProfilesPanel open />);
    });
    await flush();

    expect(ipcMocks.webEvidenceProviderDiagnostics).not.toHaveBeenCalled();
    expect(host.textContent).not.toContain("实时可用性");
    expect(host.textContent).not.toContain("测试连接");
    expect(
      Array.from(host.querySelectorAll("button")).filter(
        (item) => item.textContent?.trim() === "实时诊断",
      ),
    ).toHaveLength(1);

    await act(async () => {
      button(host, "实时诊断").click();
    });
    await flush();
    expect(ipcMocks.webEvidenceProviderDiagnostics).toHaveBeenCalledWith(
      provider.id,
    );
    expect(host.textContent).toContain("实时可用性");

    await act(async () => {
      root.render(<McpProfilesPanel open={false} />);
    });
    await flush();
    await act(async () => {
      root.render(<McpProfilesPanel open />);
    });
    await flush();
    expect(ipcMocks.webEvidenceProviderDiagnostics).toHaveBeenCalledTimes(1);
    expect(host.textContent).not.toContain("实时可用性");
  });

  it("保存、启停和编辑都会清空当前面板的诊断结果", async () => {
    await act(async () => {
      root.render(<McpProfilesPanel open />);
    });
    await flush();

    const runDiagnostics = async () => {
      await act(async () => {
        button(host, "实时诊断").click();
      });
      await flush();
      expect(host.textContent).toContain("实时可用性");
    };

    await runDiagnostics();
    await act(async () => {
      button(host, "保存 MCP 提供方").click();
    });
    await flush();
    expect(host.textContent).not.toContain("实时可用性");

    await runDiagnostics();
    await act(async () => {
      button(host, "停用").click();
    });
    await flush();
    expect(host.textContent).not.toContain("实时可用性");

    await runDiagnostics();
    await act(async () => {
      button(host, "添加凭据引用").click();
    });
    expect(host.textContent).not.toContain("实时可用性");
  });

  it("关闭面板后不会让过期的实时诊断结果复活", async () => {
    let resolveDiagnostics: (value: typeof liveDiagnostics) => void = () => {
      throw new Error("diagnostic resolver was not initialized");
    };
    ipcMocks.webEvidenceProviderDiagnostics.mockImplementationOnce(
      () =>
        new Promise<typeof liveDiagnostics>((resolve) => {
          resolveDiagnostics = resolve;
        }),
    );

    await act(async () => {
      root.render(<McpProfilesPanel open />);
    });
    await flush();

    await act(async () => {
      button(host, "实时诊断").click();
    });
    await act(async () => {
      root.render(<McpProfilesPanel open={false} />);
    });
    await flush();

    resolveDiagnostics(liveDiagnostics);
    await flush();

    expect(host.textContent).not.toContain("实时可用性");
  });

  it("清除已保存 Key 会删除加密凭据并使实时诊断失效", async () => {
    await act(async () => {
      root.render(<McpProfilesPanel open />);
    });
    await flush();

    await act(async () => {
      button(host, "清除 Key").click();
    });
    await flush();

    expect(ipcMocks.credentialDelete).toHaveBeenCalledWith(
      "iris.mcp.anysearch",
    );
    expect(host.textContent).toContain("已清除保存的 API Key");
  });

  it("拒绝把 Bearer 前缀作为 API Key 保存", async () => {
    await act(async () => {
      root.render(<McpProfilesPanel open />);
    });
    await flush();

    const input = host.querySelector<HTMLInputElement>(
      'input[type="password"]',
    );
    if (!input) throw new Error("missing API Key input");
    await act(async () => {
      const setValue = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        "value",
      )?.set;
      setValue?.call(input, "Bearer test-key");
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await act(async () => {
      button(host, "保存 MCP 提供方").click();
    });
    await flush();

    expect(ipcMocks.credentialSet).not.toHaveBeenCalled();
    expect(host.textContent).toContain("只填写原始 Key");
  });
});
