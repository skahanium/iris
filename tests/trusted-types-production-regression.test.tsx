import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ErrorBoundary } from "@/components/ErrorBoundary";
import { ConversationSurface } from "@/components/ai/ConversationSurface";
import { McpProfilesPanel } from "@/components/ai/skills/McpProfilesPanel";
import { LlmRoutingSection } from "@/components/settings/LlmRoutingSection";

const ipcMocks = vi.hoisted(() => ({
  credentialDelete: vi.fn(),
  credentialHas: vi.fn(),
  credentialStatus: vi.fn(),
  credentialSet: vi.fn(),
  llmConfigDeleteProvider: vi.fn(),
  llmConfigGet: vi.fn(),
  llmConfigSet: vi.fn(),
  llmConfigTestProvider: vi.fn(),
  llmModelRegistryRefresh: vi.fn(),
  llmModelValidate: vi.fn(),
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

const routing = {
  version: 1,
  schemaVersion: 5,
  providers: {},
  defaultModel: null,
};

const providers = [
  {
    id: "deepseek",
    name: "DeepSeek",
    default_model: "deepseek-v4-flash",
    endpointManaged: "builtin",
  },
  {
    id: "openai",
    name: "OpenAI",
    default_model: "gpt-4o-mini",
    endpointManaged: "builtin",
  },
];

describe("production TrustedHTML crash regression", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    host.style.height = "720px";
    host.style.width = "960px";
    document.body.append(host);
    root = createRoot(host);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(640);
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(420);
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      bottom: 640,
      height: 640,
      left: 0,
      right: 420,
      top: 0,
      width: 420,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });
    vi.clearAllMocks();
    ipcMocks.credentialHas.mockResolvedValue(false);
    ipcMocks.credentialStatus.mockResolvedValue({
      service: "iris.llm.deepseek",
      state: "missing",
      configured: false,
      checkedAt: "2026-07-08T00:00:00Z",
    });
    ipcMocks.llmConfigGet.mockResolvedValue({
      routing,
      providers,
      catalog: [],
      registry: [],
    });
    ipcMocks.webEvidenceProvidersList.mockResolvedValue([]);
    ipcMocks.webEvidenceProviderDiagnostics.mockResolvedValue({
      providerId: null,
      status: "disabled",
      checks: [],
    });
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.restoreAllMocks();
  });

  it("keeps the AI conversation surface mounted for empty and restored messages", async () => {
    const messageListRef = {
      current: null,
    } as React.RefObject<HTMLDivElement | null>;

    await act(async () => {
      root.render(
        <ErrorBoundary scope="AI对话区">
          <ConversationSurface
            messages={[]}
            streaming={false}
            messageListRef={messageListRef}
            onCitationClick={() => undefined}
            onQuoteToInput={() => undefined}
          />
        </ErrorBoundary>,
      );
    });

    expect(
      host.querySelector('[data-testid="ai-message-list"]'),
    ).not.toBeNull();
    expect(host.textContent).not.toContain("界面出现异常");

    await act(async () => {
      root.render(
        <ErrorBoundary scope="AI对话区">
          <ConversationSurface
            messages={[
              { role: "user", content: "今天是几月几日？" },
              {
                role: "assistant",
                content: "今天是 2026 年 7 月 5 日。",
              },
            ]}
            streaming={false}
            messageListRef={messageListRef}
            onCitationClick={() => undefined}
            onQuoteToInput={() => undefined}
          />
        </ErrorBoundary>,
      );
    });

    expect(
      host.querySelector('[data-testid="ai-message-list"]'),
    ).not.toBeNull();
    expect(host.textContent).not.toContain("界面出现异常");
  });

  it("opens the add-AI-provider wizard without tripping the root error boundary", async () => {
    await act(async () => {
      root.render(
        <ErrorBoundary>
          <LlmRoutingSection open />
        </ErrorBoundary>,
      );
    });

    const addProviderButton = Array.from(host.querySelectorAll("button")).find(
      (button) => button.textContent?.includes("添加供应商"),
    );
    await act(async () => {
      addProviderButton?.dispatchEvent(
        new MouseEvent("click", { bubbles: true }),
      );
    });

    expect(host.textContent).toContain("未配置厂商只在这里选择");
    expect(host.textContent).toContain("保存 Key");
    expect(host.textContent).not.toContain(
      "This assignment requires a TrustedHTML",
    );
    expect(host.textContent).not.toContain("界面出现异常");
  });

  it("opens the add-MCP-provider draft card without tripping the root error boundary", async () => {
    await act(async () => {
      root.render(
        <ErrorBoundary>
          <McpProfilesPanel open />
        </ErrorBoundary>,
      );
    });

    const addMcpButton = Array.from(host.querySelectorAll("button")).find(
      (button) => button.textContent?.includes("添加 MCP 提供方"),
    );
    await act(async () => {
      addMcpButton?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    expect(host.textContent).toContain("快速预设");
    expect(host.textContent).toContain("提供方名称");
    expect(host.textContent).not.toContain(
      "This assignment requires a TrustedHTML",
    );
    expect(host.textContent).not.toContain("界面出现异常");
  });
});
