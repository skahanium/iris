import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { LlmRoutingSection } from "@/components/settings/LlmRoutingSection";
import { llmConfigGet } from "@/lib/ipc";
import type { LlmConfigGetResponse, ModelCatalogEntry } from "@/types/llm";

vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true }));

vi.mock("@/lib/llm-events", () => ({
  notifyLlmConfigChanged: vi.fn(),
}));

vi.mock("@/lib/credentials", () => ({
  invokeErrorMessage: (err: unknown) =>
    err instanceof Error ? err.message : String(err),
  llmCredentialService: (provider: string) => `llm:${provider}`,
}));

vi.mock("@/lib/ipc", () => ({
  credentialDelete: vi.fn(),
  credentialStatus: vi.fn().mockResolvedValue({ configured: true }),
  credentialSet: vi.fn(),
  llmConfigDeleteProvider: vi.fn(),
  llmConfigGet: vi.fn(),
  llmConfigSet: vi.fn(),
  llmConfigTestProvider: vi.fn(),
  llmModelRegistryRefresh: vi.fn(),
  llmModelValidate: vi.fn(),
}));

const DEEPSEEK_FLASH_CATALOG: ModelCatalogEntry = {
  id: "deepseek-v4-flash",
  providerId: "deepseek",
  displayName: "DeepSeek V4 Flash",
  contextWindow: 128_000,
  maxOutput: 8192,
  supportsTools: true,
  supportsThinking: true,
  supportsVision: false,
  supportsStreaming: true,
  cacheFriendly: true,
  endpointFamily: "open_ai_compatible_chat_completions",
  probeStrategy: "open_ai_models_then_chat",
};

function deepseekConfig(options?: {
  enabledModels?: string[];
}): LlmConfigGetResponse {
  const enabledModels = options?.enabledModels ?? [];
  return {
    routing: {
      version: 1,
      schemaVersion: 6,
      providers: {
        deepseek: {
          baseUrl: null,
          label: null,
          defaultModel: "deepseek-v4-flash",
          enabledModels,
        },
      },
      candidateOrder: enabledModels.map((modelId) => ({
        providerId: "deepseek",
        modelId,
      })),
      defaultModel: null,
    },
    providers: [
      {
        id: "deepseek",
        name: "DeepSeek",
        default_model: "deepseek-v4-flash",
        endpointManaged: "builtin",
      },
    ],
    catalog: [DEEPSEEK_FLASH_CATALOG],
    registry: [],
  };
}

function renderDeepseekDetail() {
  return render(
    <LlmRoutingSection
      open
      selectedProviderId="deepseek"
      onSelectedProviderIdChange={vi.fn()}
      onProviderChromeChange={vi.fn()}
    />,
  );
}

async function typeModelAndClickAdd(modelId: string) {
  const input = await screen.findByPlaceholderText(
    "模型 ID，如 deepseek-v4-flash",
  );
  fireEvent.change(input, { target: { value: modelId } });
  fireEvent.click(screen.getByRole("button", { name: "添加模型" }));
}

describe("添加模型按钮回归（v1.2.18 状态覆盖 bug）", () => {
  beforeEach(() => {
    vi.mocked(llmConfigGet).mockResolvedValue(deepseekConfig());
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("输入模型 ID 后点击添加模型，模型出现在已启用模型列表中", async () => {
    renderDeepseekDetail();

    await typeModelAndClickAdd("deepseek-v4-flash");

    const enabledList = screen.getByTestId("llm-provider-enabled-models");
    await waitFor(() =>
      expect(enabledList.textContent).toContain("deepseek-v4-flash"),
    );
    expect(enabledList.textContent).not.toContain("未添加模型时不会激活");
  });

  it("连字符与数字组成的模型名（如 deepseek-v4-pro）可正常添加", async () => {
    renderDeepseekDetail();

    await typeModelAndClickAdd("deepseek-v4-pro");

    const enabledList = screen.getByTestId("llm-provider-enabled-models");
    await waitFor(() =>
      expect(enabledList.textContent).toContain("deepseek-v4-pro"),
    );
  });

  it("点击移除后模型从已启用模型列表中消失", async () => {
    vi.mocked(llmConfigGet).mockResolvedValue(
      deepseekConfig({ enabledModels: ["deepseek-v4-flash"] }),
    );
    renderDeepseekDetail();

    await screen.findByTestId("llm-provider-enabled-models");
    fireEvent.click(screen.getByRole("button", { name: "移除" }));

    const enabledList = screen.getByTestId("llm-provider-enabled-models");
    await waitFor(() =>
      expect(enabledList.textContent).toContain("未添加模型时不会激活"),
    );
    expect(enabledList.textContent).not.toContain("deepseek-v4-flash");
  });
});
