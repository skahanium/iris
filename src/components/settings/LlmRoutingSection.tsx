import { useCallback, useEffect, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { isTauri } from "@tauri-apps/api/core";

import { invokeErrorMessage, llmCredentialService } from "@/lib/credentials";
import {
  credentialDelete,
  credentialHas,
  credentialSet,
  llmConfigGet,
  llmConfigSet,
  llmConfigTestProvider,
  llmModelConfirmCapability,
  llmModelRegistryRefresh,
  llmModelValidate,
} from "@/lib/ipc";
import { notifyLlmConfigChanged } from "@/lib/llm-events";
import type { CapabilitySlot } from "@/types/ai";
import {
  CAPABILITY_SLOTS,
  DEFAULT_LLM_ROUTING,
  USER_CONFIGURABLE_CAPABILITY_SLOTS,
  isCustomProviderId,
  type LlmConfigGetResponse,
  type LlmRoutingConfig,
  type ModelRegistryEntry,
  type ModelValidationKind,
  type ModelCatalogEntry,
  type ProviderOverride,
  type SlotRoute,
} from "@/types/llm";

const FALLBACK_PROVIDERS: LlmConfigGetResponse["providers"] = [
  { id: "deepseek", name: "DeepSeek", default_model: "deepseek-v4-flash" },
  { id: "openai", name: "OpenAI", default_model: "gpt-4o-mini" },
  {
    id: "anthropic",
    name: "Anthropic",
    default_model: "claude-3-5-haiku-20241022",
  },
  { id: "mimo", name: "MiMo", default_model: "MiMo-V2.5-Pro" },
];

const SLOT_META: Record<
  (typeof USER_CONFIGURABLE_CAPABILITY_SLOTS)[number],
  { label: string; detail: string }
> = {
  fast: { label: "Fast", detail: "短问答、轻量检索、默认对话" },
  writer: { label: "Writer", detail: "改写、续写、章节与文档写作" },
  reasoner: { label: "Reasoner", detail: "研究、引用核查、复杂论证" },
  long_context: { label: "Long context", detail: "长文档与大上下文分析" },
  vision: { label: "Vision", detail: "图片输入与视觉问答" },
};

interface LlmRoutingSectionProps {
  open: boolean;
}

interface VisibleProvider {
  id: string;
  name: string;
  enabledModels: string[];
  usedSlots: string[];
  configured: boolean;
  keyless: boolean;
  custom: boolean;
  baseUrl: string | null;
}

interface EnabledProviderModel {
  id: string;
  catalog: ModelCatalogEntry | undefined;
  registry: ModelRegistryEntry | undefined;
}

function nextCustomProviderId(existing: Iterable<string>): string {
  const set = new Set(existing);
  if (!set.has("custom")) return "custom";
  let n = 2;
  while (set.has(`custom_${n}`)) n += 1;
  return `custom_${n}`;
}

function uniqueModelIds(models: Iterable<string>): string[] {
  const out: string[] = [];
  for (const model of models) {
    const trimmed = model.trim();
    if (trimmed && !out.includes(trimmed)) out.push(trimmed);
  }
  return out;
}

function parseModelIds(input: string): string[] {
  return uniqueModelIds(input.split(/[\n,，]/));
}

function registryKey(providerId: string, modelId: string): string {
  return `${providerId}:${modelId}`;
}

function modelMatchesCapability(
  catalog: ModelCatalogEntry | undefined,
  slot: CapabilitySlot,
): boolean {
  if (!catalog) return false;
  if (slot === "vision") return catalog.supportsVision;
  if (slot === "reasoner")
    return catalog.supportsThinking || catalog.supportsTools;
  if (slot === "long_context") return catalog.contextWindow >= 128_000;
  return slot === "fast" || slot === "writer";
}

function supportsModelForSlot(
  model: EnabledProviderModel,
  slot: CapabilitySlot,
): boolean {
  const registry = model.registry;
  if (registry?.userConfirmedCapabilities.includes(slot)) return true;
  if (modelMatchesCapability(model.catalog, slot)) return true;
  if (slot === "vision") return Boolean(registry?.visionVerifiedAt);
  if (slot === "fast" || slot === "writer") return registry?.stale !== true;
  return false;
}

function providerNeedsRefresh(entries: ModelRegistryEntry[]): boolean {
  if (entries.length === 0) return true;
  return entries.some((entry) => entry.stale || !entry.lastRefreshedAt);
}

export function LlmRoutingSection({ open }: LlmRoutingSectionProps) {
  const [data, setData] = useState<LlmConfigGetResponse | null>(null);
  const [routing, setRouting] = useState<LlmRoutingConfig | null>(null);
  const keyInputsRef = useRef<Record<string, string>>({});
  const [, setKeyInputTouch] = useState(0);
  const [keyConfigured, setKeyConfigured] = useState<Record<string, boolean>>(
    {},
  );
  const [testing, setTesting] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<
    Record<string, { ok: boolean; message: string }>
  >({});
  const [providerResults, setProviderResults] = useState<
    Record<string, { ok: boolean; message: string }>
  >({});
  const [loadError, setLoadError] = useState<string | null>(null);
  const [keysLoading, setKeysLoading] = useState(false);
  const [keySaving, setKeySaving] = useState<string | null>(null);
  const [refreshingProvider, setRefreshingProvider] = useState<string | null>(
    null,
  );
  const [wizardOpen, setWizardOpen] = useState(false);
  const [newModelInputs, setNewModelInputs] = useState<Record<string, string>>(
    {},
  );
  const [providerBaseUrlInputs, setProviderBaseUrlInputs] = useState<
    Record<string, string>
  >({});
  const routingRef = useRef<LlmRoutingConfig | null>(null);
  const keyStatusEpochRef = useRef(0);

  const applyRouting = useCallback((next: LlmRoutingConfig) => {
    routingRef.current = next;
    setRouting(next);
  }, []);

  const refreshKeyStatus = useCallback(async (providerIds: string[]) => {
    const epoch = ++keyStatusEpochRef.current;
    setKeysLoading(true);
    try {
      const configured: Record<string, boolean> = {};
      await Promise.all(
        providerIds.map(async (id) => {
          try {
            configured[id] = await credentialHas(llmCredentialService(id));
          } catch (e) {
            console.warn(`[LlmRouting] credential check failed for ${id}:`, e);
            configured[id] = false;
          }
        }),
      );
      if (epoch !== keyStatusEpochRef.current) return;
      setKeyConfigured((prev) => ({ ...prev, ...configured }));
    } finally {
      if (epoch === keyStatusEpochRef.current) {
        setKeysLoading(false);
      }
    }
  }, []);

  const load = useCallback(
    async (options?: { preserveRouting?: boolean }) => {
      setLoadError(null);
      if (!isTauri()) {
        setLoadError(
          "当前浏览器标签页无法调用 Tauri 后端，请在 Iris 桌面窗口中配置。",
        );
        const fallbackRouting = DEFAULT_LLM_ROUTING;
        applyRouting(fallbackRouting);
        setData({
          routing: fallbackRouting,
          providers: FALLBACK_PROVIDERS,
          catalog: [],
          registry: [],
        });
        return;
      }
      try {
        const res = await llmConfigGet();
        const normalized = normalizeRouting(res.routing);
        const nextRouting =
          options?.preserveRouting && routingRef.current
            ? routingRef.current
            : normalized;
        applyRouting(nextRouting);
        setData({ ...res, routing: nextRouting });
        void refreshKeyStatus(res.providers.map((p) => p.id));
      } catch (err) {
        setLoadError(invokeErrorMessage(err));
        const fallbackRouting =
          options?.preserveRouting && routingRef.current
            ? routingRef.current
            : DEFAULT_LLM_ROUTING;
        applyRouting(fallbackRouting);
        setData({
          routing: fallbackRouting,
          providers: FALLBACK_PROVIDERS,
          catalog: [],
          registry: [],
        });
      }
    },
    [applyRouting, refreshKeyStatus],
  );

  useEffect(() => {
    if (open) void load();
  }, [open, load]);

  const providerName = useCallback(
    (providerId: string) => {
      const provider = data?.providers.find((p) => p.id === providerId);
      const override = routing?.providers[providerId];
      return override?.label?.trim() || provider?.name || providerId;
    },
    [data?.providers, routing?.providers],
  );

  const modelById = (modelId: string): ModelCatalogEntry | undefined =>
    data?.catalog.find((m) => m.id === modelId);

  const baseUrlForProvider = (providerId: string): string =>
    providerBaseUrlInputs[providerId] ??
    routing?.providers[providerId]?.baseUrl ??
    "";

  const registryForProvider = (providerId: string): ModelRegistryEntry[] =>
    data?.registry.filter((entry) => entry.providerId === providerId) ?? [];

  const registryEntryForModel = (
    providerId: string,
    modelId: string,
  ): ModelRegistryEntry | undefined =>
    data?.registry.find(
      (entry) => entry.providerId === providerId && entry.modelId === modelId,
    );

  const updateProviderOverride = (
    providerId: string,
    patch: Partial<ProviderOverride>,
  ) => {
    if (!routing || !data) return;
    const prev = routing.providers[providerId] ?? {
      baseUrl: null,
      label: null,
      defaultModel: null,
      enabledModels: null,
    };
    const next: ProviderOverride = { ...prev, ...patch };
    const nextRouting = {
      ...routing,
      providers: { ...routing.providers, [providerId]: next },
    };
    applyRouting(nextRouting);
    setData({
      ...data,
      routing: nextRouting,
      providers: data.providers.map((p) =>
        p.id === providerId
          ? {
              ...p,
              name:
                next.label?.trim() ||
                (isCustomProviderId(providerId)
                  ? `Custom (${providerId})`
                  : p.name),
              default_model: next.defaultModel?.trim() || p.default_model,
            }
          : p,
      ),
    });
  };

  const updateProviderBaseUrl = (providerId: string, value: string) => {
    setProviderBaseUrlInputs((prev) => ({ ...prev, [providerId]: value }));
    updateProviderOverride(providerId, { baseUrl: value.trim() || null });
  };

  const ensureCustomProvider = () => {
    if (!routing || !data) return null;
    const id = nextCustomProviderId([
      ...Object.keys(routing.providers),
      ...data.providers.map((p) => p.id),
    ]);
    const label = `自定义端点 ${
      data.providers.filter((p) => isCustomProviderId(p.id)).length + 1
    }`;
    const entry: ProviderOverride = {
      baseUrl: null,
      label,
      defaultModel: null,
      enabledModels: [],
    };
    const nextRouting = {
      ...routing,
      providers: { ...routing.providers, [id]: entry },
    };
    applyRouting(nextRouting);
    setData({
      ...data,
      routing: nextRouting,
      providers: [...data.providers, { id, name: label, default_model: "" }],
    });
    void refreshKeyStatus([id]);
    return id;
  };

  const saveKey = async (providerId: string) => {
    const value = keyInputsRef.current[providerId]?.trim();
    if (!value) return;
    const service = llmCredentialService(providerId);
    const label = providerName(providerId);

    keyStatusEpochRef.current += 1;
    setKeySaving(providerId);
    setMessage(null);
    try {
      await credentialSet(service, value);
      keyInputsRef.current[providerId] = "";
      setKeyInputTouch((n) => n + 1);
      setKeyConfigured((prev) => ({ ...prev, [providerId]: true }));
      setMessage(`${label} Key 已保存到系统凭据管理器。`);
      notifyLlmConfigChanged();
    } catch (err) {
      setMessage(`保存 ${label} Key 失败：${invokeErrorMessage(err)}`);
    } finally {
      setKeySaving(null);
    }
  };

  const clearKey = async (providerId: string) => {
    const label = providerName(providerId);
    keyStatusEpochRef.current += 1;
    try {
      await credentialDelete(llmCredentialService(providerId));
      setKeyConfigured((prev) => ({ ...prev, [providerId]: false }));
      setMessage(`${label} Key 已清除`);
      notifyLlmConfigChanged();
    } catch (err) {
      setMessage(`清除 ${label} Key 失败：${invokeErrorMessage(err)}`);
    }
  };

  const updateSlot = (
    slot: CapabilitySlot,
    patch: Partial<{ providerId: string; model: string; thinking: boolean }>,
  ) => {
    if (!routing) return;
    const current = routing.slots[slot] ?? DEFAULT_LLM_ROUTING.slots[slot];
    const nextRoute = { ...current, ...patch };
    const nextSlots: LlmRoutingConfig["slots"] = {
      ...routing.slots,
      [slot]: nextRoute,
    };
    const agentTools = routing.slots.agent_tools;
    const reasoner =
      routing.slots.reasoner ?? DEFAULT_LLM_ROUTING.slots.reasoner;
    if (!agentTools || sameRoute(agentTools, reasoner)) {
      nextSlots.agent_tools =
        slot === "reasoner" ? nextRoute : (nextSlots.agent_tools ?? reasoner);
    }
    applyRouting({
      ...routing,
      slots: nextSlots,
    });
  };

  const saveRouting = async () => {
    if (!routing) return;
    setSaving(true);
    setMessage(null);
    try {
      await llmConfigSet(routing);
      setMessage("能力槽路由已保存");
      notifyLlmConfigChanged();
    } finally {
      setSaving(false);
    }
  };

  const enabledModelIdsForProvider = (providerId: string): string[] => {
    if (!routing) return [];
    const override = routing.providers[providerId];
    return uniqueModelIds(override?.enabledModels ?? []);
  };

  const enabledModelsForProvider = (
    providerId: string,
  ): EnabledProviderModel[] => {
    const enabled = enabledModelIdsForProvider(providerId);
    return enabled.map((modelId) => ({
      id: modelId,
      catalog: modelById(modelId),
      registry: registryEntryForModel(providerId, modelId),
    }));
  };

  const modelsForSlot = (
    slot: CapabilitySlot,
    providerId: string,
  ): EnabledProviderModel[] =>
    enabledModelsForProvider(providerId).filter((model) =>
      supportsModelForSlot(model, slot),
    );

  const modelUsageLabels = (providerId: string, modelId: string) =>
    USER_CONFIGURABLE_CAPABILITY_SLOTS.filter((slot) => {
      const route = routing?.slots[slot] ?? DEFAULT_LLM_ROUTING.slots[slot];
      return route.providerId === providerId && route.model === modelId;
    }).map((slot) => SLOT_META[slot].label);

  const addProviderModel = (providerId: string) => {
    if (!routing) return;
    const additions = parseModelIds(newModelInputs[providerId] ?? "");
    if (additions.length === 0) return;
    const enabled = enabledModelIdsForProvider(providerId);
    const nextEnabled = uniqueModelIds([...enabled, ...additions]);
    updateProviderOverride(providerId, {
      enabledModels: nextEnabled,
      defaultModel:
        routing.providers[providerId]?.defaultModel ?? nextEnabled[0],
    });
    setNewModelInputs((prev) => ({ ...prev, [providerId]: "" }));
  };

  const removeProviderModel = (providerId: string, modelId: string) => {
    if (!routing) return;
    const usage = modelUsageLabels(providerId, modelId);
    if (usage.length > 0) {
      setMessage(
        `模型 ${modelId} 正在用于 ${usage.join(" / ")}，请先调整路由。`,
      );
      return;
    }
    const enabled = enabledModelIdsForProvider(providerId);
    const nextEnabled = enabled.filter((id) => id !== modelId);
    updateProviderOverride(providerId, {
      enabledModels: nextEnabled,
      defaultModel:
        routing.providers[providerId]?.defaultModel === modelId
          ? (nextEnabled[0] ?? null)
          : routing.providers[providerId]?.defaultModel,
    });
  };

  const visibleProviders = (() => {
    if (!routing || !data) return [];
    const providers = new Map<string, VisibleProvider>();
    const addProvider = (
      providerId: string,
      usage: string | null,
      configuredOverride = false,
    ) => {
      const override = routing.providers[providerId];
      const row = providers.get(providerId) ?? {
        id: providerId,
        name: providerName(providerId),
        enabledModels: enabledModelIdsForProvider(providerId),
        usedSlots: [],
        configured:
          configuredOverride ||
          Boolean(keyConfigured[providerId]) ||
          Boolean(override),
        keyless: false,
        custom: isCustomProviderId(providerId),
        baseUrl: baseUrlForProvider(providerId) || null,
      };
      if (usage && !row.usedSlots.includes(usage)) row.usedSlots.push(usage);
      providers.set(providerId, row);
    };

    for (const slot of USER_CONFIGURABLE_CAPABILITY_SLOTS) {
      const route = routing.slots[slot] ?? DEFAULT_LLM_ROUTING.slots[slot];
      addProvider(route.providerId, SLOT_META[slot].label);
    }

    for (const provider of data.providers) {
      const override = routing.providers[provider.id];
      const configured =
        Boolean(keyConfigured[provider.id]) || Boolean(override);
      if (!configured) continue;
      addProvider(provider.id, null, configured);
    }

    return Array.from(providers.values()).sort((a, b) => {
      const score = (provider: VisibleProvider) =>
        provider.usedSlots.length > 0 ? 0 : provider.configured ? 1 : 2;
      return score(a) - score(b) || a.name.localeCompare(b.name);
    });
  })();

  const testProvider = async (provider: VisibleProvider) => {
    setTesting(provider.id);
    setProviderResults((prev) => {
      const next = { ...prev };
      delete next[provider.id];
      return next;
    });
    try {
      const result = await llmConfigTestProvider(provider.id);
      setProviderResults((prev) => ({ ...prev, [provider.id]: result }));
    } catch (err) {
      setProviderResults((prev) => ({
        ...prev,
        [provider.id]: { ok: false, message: invokeErrorMessage(err) },
      }));
    } finally {
      setTesting(null);
    }
  };

  const refreshProviderModels = async (provider: VisibleProvider) => {
    setRefreshingProvider(provider.id);
    try {
      const result = await llmModelRegistryRefresh(provider.id);
      setMessage(result.message);
      await load({ preserveRouting: true });
    } catch (err) {
      setMessage(invokeErrorMessage(err));
    } finally {
      setRefreshingProvider(null);
    }
  };

  const validateProviderModel = async (
    provider: VisibleProvider,
    model: EnabledProviderModel,
    kind: ModelValidationKind,
  ) => {
    const key = `${provider.id}:${model.id}`;
    if (provider.id === "mimo" && !baseUrlForProvider(provider.id).trim()) {
      setTestResults((prev) => ({
        ...prev,
        [key]: {
          ok: false,
          message: "MiMo 需配置 Base URL 后才能测试连接。",
        },
      }));
      return;
    }
    setTesting(key);
    setTestResults((prev) => {
      const next = { ...prev };
      delete next[key];
      return next;
    });
    try {
      const result = await llmModelValidate(provider.id, model.id, kind);
      setTestResults((prev) => ({ ...prev, [key]: result }));
      if (result.ok) await load({ preserveRouting: true });
    } catch (err) {
      setTestResults((prev) => ({
        ...prev,
        [key]: { ok: false, message: invokeErrorMessage(err) },
      }));
    } finally {
      setTesting(null);
    }
  };

  const confirmProviderModelCapability = async (
    provider: VisibleProvider,
    model: EnabledProviderModel,
    slot: CapabilitySlot,
  ) => {
    try {
      await llmModelConfirmCapability({
        providerId: provider.id,
        modelId: model.id,
        slot,
      });
      await load({ preserveRouting: true });
    } catch (err) {
      setMessage(invokeErrorMessage(err));
    }
  };

  if (!routing || !data) {
    return (
      <div className="space-y-2" data-section="ai-connection">
        <p className="text-xs text-muted-foreground">加载 AI 连接配置…</p>
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="h-7 text-xs"
          onClick={() => void load()}
        >
          重试
        </Button>
      </div>
    );
  }

  return (
    <div className="space-y-5" data-section="ai-connection">
      <div>
        <h3 className="text-sm font-medium">模型与供应商</h3>
        <p className="mt-0.5 text-xs text-muted-foreground">
          供应商只保存 API
          与端点；模型由你手动填写，未添加模型时不会激活或展示任何模型。
        </p>
        {loadError ? (
          <p className="mt-2 text-xs text-amber-600">
            未能从后端读取配置：{loadError}
          </p>
        ) : null}
        {keysLoading ? (
          <p className="mt-1 text-[10px] text-muted-foreground">
            正在检查已配置凭据…
          </p>
        ) : null}
      </div>

      <section className="space-y-2" data-section="llm-providers">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="text-xs font-medium text-muted-foreground">
            供应商配置
          </p>
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="h-7 text-xs"
            onClick={() => setWizardOpen((value) => !value)}
          >
            添加供应商
          </Button>
        </div>

        {wizardOpen ? (
          <AddModelWizard
            providers={data.providers}
            keyConfigured={keyConfigured}
            keyInputsRef={keyInputsRef}
            keySaving={keySaving}
            onKeyInput={(id, value) => {
              keyInputsRef.current[id] = value;
              setKeyInputTouch((n) => n + 1);
            }}
            onSaveKey={(id) => void saveKey(id)}
            onCreateCustom={ensureCustomProvider}
            onBaseUrl={(id, url) =>
              updateProviderOverride(id, { baseUrl: url.trim() || null })
            }
            onLabel={(id, label) =>
              updateProviderOverride(id, { label: label.trim() || null })
            }
            onClose={() => setWizardOpen(false)}
          />
        ) : null}

        {visibleProviders.length === 0 ? (
          <p className="rounded-md border border-border/50 bg-background/60 px-3 py-3 text-xs text-muted-foreground">
            暂无已配置供应商。点击“添加供应商”保存 Key 或配置本地端点。
          </p>
        ) : (
          <div className="space-y-2">
            {visibleProviders.map((provider) => {
              const providerBaseUrl = baseUrlForProvider(provider.id);
              const missingRequiredBaseUrl =
                provider.id === "mimo" && !providerBaseUrl.trim();
              const override = routing.providers[provider.id];
              const providerModels = enabledModelsForProvider(provider.id);
              const providerResult = providerResults[provider.id];
              return (
                <div
                  key={provider.id}
                  data-testid="llm-provider-card"
                  className="grid gap-3 rounded-md border border-border/55 bg-background/60 p-3 xl:grid-cols-[minmax(14rem,0.85fr)_minmax(18rem,1.4fr)]"
                >
                  <div className="min-w-0 space-y-2">
                    <p className="truncate text-xs font-medium text-foreground">
                      {provider.name}
                    </p>
                    <p className="text-[11px] text-muted-foreground">
                      {provider.usedSlots.length > 0
                        ? `用于 ${provider.usedSlots.join(" / ")}`
                        : `${provider.enabledModels.length} 个模型已启用`}
                    </p>
                    {provider.custom ? (
                      <Input
                        className="h-8 text-xs"
                        placeholder="显示名称"
                        defaultValue={override?.label ?? provider.name}
                        onBlur={(event) =>
                          updateProviderOverride(provider.id, {
                            label: event.target.value.trim() || null,
                          })
                        }
                      />
                    ) : null}
                    <Input
                      className="h-8 text-xs"
                      placeholder={
                        provider.keyless
                          ? "本地端点 Base URL（可选）"
                          : "Base URL（可选）"
                      }
                      value={baseUrlForProvider(provider.id)}
                      onChange={(event) =>
                        updateProviderBaseUrl(provider.id, event.target.value)
                      }
                    />
                    <div className="flex flex-wrap items-center gap-2">
                      {!provider.keyless ? (
                        <>
                          <Input
                            type="password"
                            className="h-8 w-44 text-xs"
                            placeholder="API Key…"
                            value={keyInputsRef.current?.[provider.id] ?? ""}
                            onChange={(event) => {
                              keyInputsRef.current[provider.id] =
                                event.target.value;
                              setKeyInputTouch((n) => n + 1);
                            }}
                          />
                          <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            className="h-8"
                            disabled={keySaving === provider.id}
                            onClick={() => void saveKey(provider.id)}
                          >
                            保存 Key
                          </Button>
                          {keyConfigured[provider.id] ? (
                            <Button
                              type="button"
                              size="sm"
                              variant="ghost"
                              className="h-8"
                              onClick={() => void clearKey(provider.id)}
                            >
                              清除
                            </Button>
                          ) : null}
                        </>
                      ) : null}
                    </div>
                    <p className="text-[11px] text-muted-foreground">
                      {missingRequiredBaseUrl
                        ? "需配置 Base URL"
                        : provider.keyless
                          ? "本地端点"
                          : provider.configured
                            ? "Key 已配置"
                            : "需要配置 Key"}
                    </p>
                    <div className="flex flex-wrap items-center gap-2">
                      <Button
                        type="button"
                        size="sm"
                        variant="secondary"
                        className="h-7 text-xs"
                        disabled={testing === provider.id}
                        onClick={() => void testProvider(provider)}
                      >
                        {testing === provider.id ? "测试中…" : "测试连接"}
                      </Button>
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        className="h-7 text-xs"
                        disabled={refreshingProvider === provider.id}
                        onClick={() => void refreshProviderModels(provider)}
                      >
                        {refreshingProvider === provider.id
                          ? "刷新中…"
                          : "刷新模型"}
                      </Button>
                    </div>
                    {providerResult ? (
                      <p
                        className={
                          providerResult.ok
                            ? "text-[11px] text-emerald-600"
                            : "text-[11px] text-destructive"
                        }
                      >
                        {providerResult.message}
                      </p>
                    ) : null}
                  </div>

                  <div
                    className="space-y-2"
                    data-testid="llm-provider-enabled-models"
                  >
                    <div className="flex flex-wrap items-center gap-2">
                      <Input
                        className="h-8 min-w-48 flex-1 text-xs"
                        placeholder="模型 ID，如 deepseek-v4-flash"
                        autoCapitalize="none"
                        autoCorrect="off"
                        spellCheck={false}
                        value={newModelInputs[provider.id] ?? ""}
                        onChange={(event) =>
                          setNewModelInputs((prev) => ({
                            ...prev,
                            [provider.id]: event.target.value,
                          }))
                        }
                        onKeyDown={(event) => {
                          if (event.key === "Enter") {
                            event.preventDefault();
                            addProviderModel(provider.id);
                          }
                        }}
                      />
                      <Button
                        type="button"
                        size="sm"
                        variant="secondary"
                        className="h-8 text-xs"
                        onClick={() => addProviderModel(provider.id)}
                      >
                        添加模型
                      </Button>
                    </div>
                    <p className="text-[11px] text-muted-foreground">
                      可一次粘贴多个模型 ID，用逗号或换行分隔；同一个 Key
                      会被这些模型共享。
                    </p>
                    {providerModels.length === 0 ? (
                      <p className="rounded-md border border-dashed border-border/50 px-3 py-2 text-[11px] text-muted-foreground">
                        未添加模型时不会激活或展示任何模型。
                      </p>
                    ) : (
                      providerModels.map((model) => {
                        const key = registryKey(provider.id, model.id);
                        const result = testResults[key];
                        const usage = modelUsageLabels(provider.id, model.id);
                        return (
                          <div
                            key={model.id}
                            className="rounded-md border border-border/45 bg-background/50 p-2"
                          >
                            <div className="flex flex-wrap items-start justify-between gap-2">
                              <div className="flex min-w-0 flex-1 items-start gap-2">
                                <span className="min-w-0">
                                  <span className="block truncate font-mono text-xs font-medium text-foreground">
                                    {model.id}
                                  </span>
                                  {model.catalog?.displayName ? (
                                    <span className="block truncate text-[11px] text-muted-foreground">
                                      {model.catalog.displayName}
                                    </span>
                                  ) : null}
                                  {usage.length > 0 ? (
                                    <span className="block text-[11px] text-muted-foreground">
                                      用于 {usage.join(" / ")}
                                    </span>
                                  ) : null}
                                </span>
                              </div>
                              <div className="flex items-center gap-2">
                                <Button
                                  type="button"
                                  size="sm"
                                  variant="secondary"
                                  className="h-7 text-xs"
                                  disabled={
                                    testing === key || missingRequiredBaseUrl
                                  }
                                  onClick={() =>
                                    void validateProviderModel(
                                      provider,
                                      model,
                                      "text",
                                    )
                                  }
                                >
                                  {testing === key ? "诊断中…" : "诊断"}
                                </Button>
                                <Button
                                  type="button"
                                  size="sm"
                                  variant="ghost"
                                  className="h-7 text-xs"
                                  onClick={() =>
                                    removeProviderModel(provider.id, model.id)
                                  }
                                >
                                  移除
                                </Button>
                              </div>
                            </div>
                            <div className="mt-2">
                              <CapabilityTags model={model.catalog} />
                            </div>
                            {result ? (
                              <p
                                className={
                                  result.ok
                                    ? "mt-2 text-[11px] text-emerald-600"
                                    : "mt-2 text-[11px] text-destructive"
                                }
                              >
                                {result.message}
                              </p>
                            ) : null}
                          </div>
                        );
                      })
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </section>

      <section className="space-y-2" data-section="llm-model-catalog">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="text-xs font-medium text-muted-foreground">
            模型目录与能力验证
          </p>
        </div>
        <div className="space-y-2">
          {visibleProviders.map((provider) => {
            const entries = registryForProvider(provider.id);
            const models = enabledModelsForProvider(provider.id);
            return (
              <div
                key={provider.id}
                className="rounded-md border border-border/50 bg-background/60 p-3"
              >
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="min-w-0">
                    <p className="truncate text-xs font-medium text-foreground">
                      {provider.name}
                    </p>
                    <p className="text-[11px] text-muted-foreground">
                      {entries.length} 个目录模型，{models.length} 个已启用模型
                    </p>
                  </div>
                  {providerNeedsRefresh(entries) ? (
                    <span className="text-[11px] text-amber-600">
                      建议刷新目录
                    </span>
                  ) : null}
                </div>
                {models.length === 0 ? (
                  <p className="mt-2 rounded-md border border-dashed border-border/50 px-3 py-2 text-[11px] text-muted-foreground">
                    未添加模型时不会激活或展示任何模型。
                  </p>
                ) : (
                  <div className="mt-2 grid gap-2 lg:grid-cols-2">
                    {models.map((model) => {
                      const key = registryKey(provider.id, model.id);
                      const result = testResults[key];
                      return (
                        <div
                          key={model.id}
                          className="rounded-md border border-border/45 bg-background/50 p-2"
                        >
                          <div className="flex flex-wrap items-start justify-between gap-2">
                            <div className="min-w-0">
                              <p className="truncate font-mono text-xs font-medium text-foreground">
                                {model.id}
                              </p>
                              {model.catalog?.displayName ||
                              model.registry?.displayName ? (
                                <p className="truncate text-[11px] text-muted-foreground">
                                  {model.catalog?.displayName ??
                                    model.registry?.displayName}
                                </p>
                              ) : null}
                            </div>
                            <div className="flex flex-wrap items-center gap-1.5">
                              <Button
                                type="button"
                                size="sm"
                                variant="secondary"
                                className="h-7 text-xs"
                                disabled={testing === key}
                                onClick={() =>
                                  void validateProviderModel(
                                    provider,
                                    model,
                                    "text",
                                  )
                                }
                              >
                                文本验证
                              </Button>
                              <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                className="h-7 text-xs"
                                disabled={testing === key}
                                onClick={() =>
                                  void validateProviderModel(
                                    provider,
                                    model,
                                    "vision",
                                  )
                                }
                              >
                                视觉验证
                              </Button>
                            </div>
                          </div>
                          <div className="mt-2">
                            <CapabilityTags model={model.catalog} />
                          </div>
                          <div className="mt-2 flex flex-wrap gap-1.5">
                            {USER_CONFIGURABLE_CAPABILITY_SLOTS.map((slot) => (
                              <Button
                                key={slot}
                                type="button"
                                size="sm"
                                variant={
                                  supportsModelForSlot(model, slot)
                                    ? "secondary"
                                    : "ghost"
                                }
                                className="h-6 px-2 text-[10px]"
                                onClick={() =>
                                  void confirmProviderModelCapability(
                                    provider,
                                    model,
                                    slot,
                                  )
                                }
                              >
                                {SLOT_META[slot].label}
                              </Button>
                            ))}
                          </div>
                          {result ? (
                            <p
                              className={
                                result.ok
                                  ? "mt-2 text-[11px] text-emerald-600"
                                  : "mt-2 text-[11px] text-destructive"
                              }
                            >
                              {result.message}
                            </p>
                          ) : null}
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </section>

      <section className="space-y-2" data-section="llm-capability-routing">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="text-xs font-medium text-muted-foreground">
            能力槽模型路由
          </p>
        </div>

        <div className="space-y-2">
          {USER_CONFIGURABLE_CAPABILITY_SLOTS.map((slot) => {
            const route =
              routing.slots[slot] ?? DEFAULT_LLM_ROUTING.slots[slot];
            const providerId = route.providerId;
            const models = modelsForSlot(slot, providerId);
            const catalogModel = modelById(route.model);
            return (
              <div
                key={slot}
                className="grid gap-2 rounded-md border border-border/50 bg-background/60 p-2 xl:grid-cols-[minmax(8rem,0.85fr)_1fr_1.2fr_1.5fr]"
              >
                <div className="min-w-0 self-center">
                  <p className="text-xs font-medium text-foreground">
                    {SLOT_META[slot].label}
                  </p>
                  <p className="mt-0.5 text-[11px] text-muted-foreground">
                    {SLOT_META[slot].detail}
                  </p>
                </div>
                <Select
                  value={providerId}
                  onValueChange={(value) =>
                    updateSlot(slot, {
                      providerId: value,
                      model: modelsForSlot(slot, value)[0]?.id ?? "",
                    })
                  }
                >
                  <SelectTrigger className="h-8 text-xs">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {data.providers.map((p) => (
                      <SelectItem key={p.id} value={p.id}>
                        {p.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                {models.length === 0 ? (
                  <Input
                    className="h-8 text-xs"
                    placeholder="先在供应商配置中添加模型"
                    value=""
                    disabled
                  />
                ) : (
                  <Select
                    value={route.model}
                    onValueChange={(value) =>
                      updateSlot(slot, { model: value })
                    }
                  >
                    <SelectTrigger className="h-8 text-xs">
                      <SelectValue placeholder="模型" />
                    </SelectTrigger>
                    <SelectContent>
                      {models.map((model) => (
                        <SelectItem key={model.id} value={model.id}>
                          {model.id}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                )}
                <CapabilityTags model={catalogModel} />
              </div>
            );
          })}
        </div>
      </section>

      <div className="flex items-center gap-2">
        <Button
          type="button"
          size="sm"
          disabled={saving || Boolean(loadError)}
          onClick={() => void saveRouting()}
        >
          {saving ? "保存中…" : "保存路由"}
        </Button>
        {message ? (
          <span className="text-xs text-muted-foreground">{message}</span>
        ) : null}
      </div>
    </div>
  );
}

function AddModelWizard({
  providers,
  keyConfigured,
  keyInputsRef,
  keySaving,
  onKeyInput,
  onSaveKey,
  onCreateCustom,
  onBaseUrl,
  onLabel,
  onClose,
}: {
  providers: LlmConfigGetResponse["providers"];
  keyConfigured: Record<string, boolean>;
  keyInputsRef: React.RefObject<Record<string, string>>;
  keySaving: string | null;
  onKeyInput: (id: string, value: string) => void;
  onSaveKey: (id: string) => void;
  onCreateCustom: () => string | null;
  onBaseUrl: (id: string, url: string) => void;
  onLabel: (id: string, label: string) => void;
  onClose: () => void;
}) {
  const [providerId, setProviderId] = useState(providers[0]?.id ?? "deepseek");
  const custom = isCustomProviderId(providerId);

  const createCustom = () => {
    const id = onCreateCustom();
    if (id) setProviderId(id);
  };

  return (
    <div className="rounded-md border border-border/60 bg-surface-inset/30 p-3">
      <div className="flex items-center justify-between gap-2">
        <div>
          <p className="text-xs font-semibold text-foreground">添加供应商</p>
          <p className="mt-1 text-[11px] text-muted-foreground">
            未配置厂商只在这里选择；保存后才进入主列表。
          </p>
        </div>
        <Button type="button" size="sm" variant="ghost" onClick={onClose}>
          收起
        </Button>
      </div>
      <div className="mt-3 grid gap-2 lg:grid-cols-[1fr_auto]">
        <Select value={providerId} onValueChange={setProviderId}>
          <SelectTrigger className="h-8 text-xs">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {providers.map((p) => (
              <SelectItem key={p.id} value={p.id}>
                {p.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="h-8"
          onClick={createCustom}
        >
          自定义端点
        </Button>
      </div>
      {custom ? (
        <div className="mt-2 grid gap-2 lg:grid-cols-2">
          <Input
            className="h-8 text-xs"
            placeholder="显示名称"
            onBlur={(event) => onLabel(providerId, event.target.value)}
          />
          <Input
            className="h-8 text-xs"
            placeholder="Base URL"
            onBlur={(event) => onBaseUrl(providerId, event.target.value)}
          />
        </div>
      ) : null}
      <div className="mt-2 flex flex-wrap items-center gap-2">
        <Input
          type="password"
          className="h-8 max-w-sm text-xs"
          placeholder="API Key…"
          value={keyInputsRef.current?.[providerId] ?? ""}
          onChange={(event) => onKeyInput(providerId, event.target.value)}
        />
        <Button
          type="button"
          size="sm"
          className="h-8"
          disabled={keySaving === providerId}
          onClick={() => onSaveKey(providerId)}
        >
          {keySaving === providerId ? "保存中…" : "保存 Key"}
        </Button>
        <span className="text-[11px] text-muted-foreground">
          {keyConfigured[providerId] ? "Key 已配置" : "保存后显示在主列表"}
        </span>
      </div>
    </div>
  );
}

function CapabilityTags({ model }: { model: ModelCatalogEntry | undefined }) {
  if (!model) {
    return (
      <div className="flex flex-wrap items-center gap-1 text-[10px] text-muted-foreground">
        <span className="rounded border border-border/50 px-1.5 py-0.5">
          manual model
        </span>
      </div>
    );
  }
  const tags = [
    model.supportsVision ? "vision" : null,
    model.supportsTools ? "tools" : null,
    model.supportsStreaming ? "streaming" : null,
    model.supportsThinking ? "reasoning" : null,
    `${Math.round(model.contextWindow / 1000)}k ctx`,
    model.endpointFamily,
  ].filter((tag): tag is string => Boolean(tag));

  return (
    <div className="flex flex-wrap items-center gap-1">
      {tags.map((tag) => (
        <span
          key={tag}
          className="rounded border border-border/50 px-1.5 py-0.5 text-[10px] text-muted-foreground"
        >
          {tag}
        </span>
      ))}
    </div>
  );
}

function normalizeRouting(raw: LlmRoutingConfig | undefined): LlmRoutingConfig {
  if (!raw) return DEFAULT_LLM_ROUTING;
  const providers: LlmRoutingConfig["providers"] = {};
  for (const [id, provider] of Object.entries(raw.providers ?? {})) {
    const row = provider as ProviderOverride & {
      base_url?: string | null;
      default_model?: string | null;
      enabled_models?: string[] | null;
    };
    providers[id] = {
      baseUrl: row.baseUrl ?? row.base_url ?? null,
      label: row.label ?? null,
      defaultModel: row.defaultModel ?? row.default_model ?? null,
      enabledModels: row.enabledModels ?? row.enabled_models ?? null,
    };
  }

  const slots: LlmRoutingConfig["slots"] = { ...DEFAULT_LLM_ROUTING.slots };
  const legacyScenes = (
    raw as LlmRoutingConfig & {
      scenes?: Record<string, SlotRoute & { provider_id?: string }>;
    }
  ).scenes;
  const legacySceneToSlot: Partial<Record<CapabilitySlot, string>> = {
    fast: "knowledge_lookup",
    writer: "drafting_assist",
    reasoner: "research_synthesis",
    long_context: "exemplar_learning",
    agent_tools: "knowledge_lookup",
  };
  for (const [slot, scene] of Object.entries(legacySceneToSlot)) {
    const route = legacyScenes?.[scene];
    if (!route) continue;
    slots[slot as CapabilitySlot] = {
      providerId: route.providerId ?? route.provider_id ?? "deepseek",
      model: normalizePersistedModelId(
        route.model ?? DEFAULT_LLM_ROUTING.slots[slot as CapabilitySlot].model,
      ),
      thinking: route.thinking ?? false,
    };
  }
  for (const slot of CAPABILITY_SLOTS) {
    const rawSlots = raw.slots as Partial<Record<CapabilitySlot, SlotRoute>>;
    const route = rawSlots?.[slot];
    if (!route) continue;
    const row = route as SlotRoute & { provider_id?: string };
    slots[slot] = {
      providerId: row.providerId ?? row.provider_id ?? "deepseek",
      model: normalizePersistedModelId(
        row.model ?? DEFAULT_LLM_ROUTING.slots[slot].model,
      ),
      thinking: row.thinking ?? false,
    };
  }

  return {
    version: raw.version ?? 1,
    schemaVersion: raw.schemaVersion ?? 2,
    providers,
    slots,
    contextStrategy: raw.contextStrategy ?? DEFAULT_LLM_ROUTING.contextStrategy,
  };
}

function sameRoute(a: SlotRoute, b: SlotRoute): boolean {
  return (
    a.providerId === b.providerId &&
    a.model === b.model &&
    Boolean(a.thinking) === Boolean(b.thinking)
  );
}

function normalizePersistedModelId(model: string): string {
  return model === "mimo-vl-7b-experimental" ? "MiMo-V2.5-Pro" : model;
}
