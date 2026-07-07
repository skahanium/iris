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
  llmConfigDeleteProvider,
  llmConfigGet,
  llmConfigSet,
  llmConfigTestProvider,
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
  type ModelCatalogEntry,
  type ReasoningControl,
  type ReasoningMode,
  type ReasoningSlotConfig,
  type ProviderOverride,
  type SlotRoute,
} from "@/types/llm";

const FALLBACK_PROVIDERS: LlmConfigGetResponse["providers"] = [
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
  {
    id: "anthropic",
    name: "Anthropic",
    default_model: "claude-3-5-haiku-20241022",
    endpointManaged: "builtin",
  },
  {
    id: "mimo",
    name: "MiMo",
    default_model: "MiMo-V2.5-Pro",
    endpointManaged: "builtin",
  },
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

const REASONING_LABELS: Record<ReasoningMode, string> = {
  off: "关闭",
  auto: "自动",
  minimal: "极简",
  low: "低",
  medium: "中",
  high: "高",
  xhigh: "极高",
};

const REASONING_STRENGTH_OPTIONS: ReasoningMode[] = [
  "off",
  "auto",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
];

const REASONING_EFFORT_OPTIONS: ReasoningMode[] = [
  "off",
  "auto",
  "low",
  "medium",
  "high",
];

const REASONING_SWITCH_OPTIONS: ReasoningMode[] = ["off", "auto"];

const UNSUPPORTED_REASONING_CAPABILITY: ReasoningUiCapability = {
  supported: false,
  control: "none",
  tagOnly: false,
  supportedModes: [],
  defaultMode: "off",
  disableSupported: true,
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
  custom: boolean;
  endpointManaged: "builtin" | "custom";
}

interface EnabledProviderModel {
  id: string;
  catalog: ModelCatalogEntry | undefined;
  registry: ModelRegistryEntry | undefined;
}

interface ReasoningUiCapability {
  supported: boolean;
  control: ReasoningControl;
  tagOnly: boolean;
  supportedModes: ReasoningMode[];
  defaultMode: ReasoningMode;
  disableSupported: boolean;
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

function findModelCatalogForProvider(
  catalog: ModelCatalogEntry[] | undefined,
  providerId: string,
  modelId: string,
): ModelCatalogEntry | undefined {
  return catalog?.find(
    (model) => model.providerId === providerId && model.id === modelId,
  );
}

function textValidatedModel(model: EnabledProviderModel): boolean {
  return Boolean(
    model.catalog ||
    model.registry?.textVerifiedAt ||
    model.registry?.visionVerifiedAt,
  );
}

function modelSupportsSlot(
  model: EnabledProviderModel,
  slot: CapabilitySlot,
): boolean {
  if (slot === "vision") {
    if (model.catalog) return model.catalog.supportsVision;
    return Boolean(model.registry?.visionVerifiedAt);
  }
  if (
    slot === "fast" ||
    slot === "writer" ||
    slot === "reasoner" ||
    slot === "long_context"
  ) {
    return textValidatedModel(model);
  }
  return false;
}

function modelCapabilitySummary(
  model: EnabledProviderModel,
  result: { ok: boolean; message: string } | undefined,
): string {
  if (result) return result.message;
  const textReady = textValidatedModel(model);
  if (!textReady) return "未验证";
  const visionReady = modelSupportsSlot(model, "vision");
  return visionReady ? "文本可用 · 视觉可用" : "文本可用 · 视觉不支持";
}

function normalizeReasoningSlot(
  route: Pick<SlotRoute, "thinking" | "reasoning"> | undefined,
): ReasoningSlotConfig {
  if (route?.reasoning?.mode) return route.reasoning;
  return { mode: route?.thinking ? "auto" : "off" };
}

function modelLooksTagReasoningRisk(
  providerId: string,
  modelId: string,
): boolean {
  const provider = providerId.toLowerCase();
  return (
    provider.includes("minimax") ||
    /minimax/i.test(modelId) ||
    /^minimax-m3$/i.test(modelId)
  );
}

function modelLooksOpenAiReasoning(
  providerId: string,
  modelId: string,
): boolean {
  const provider = providerId.toLowerCase();
  return provider === "openai" && /^(o1|o3|o4|gpt-5)/i.test(modelId);
}

function modelLooksGlmReasoning(providerId: string, modelId: string): boolean {
  const provider = providerId.toLowerCase();
  return provider === "zhipu" && /^(glm-4\.5|glm-5)/i.test(modelId);
}

function modelLooksQwenReasoning(providerId: string, modelId: string): boolean {
  const provider = providerId.toLowerCase();
  return (
    provider.includes("qwen") ||
    provider.includes("dashscope") ||
    /qwen3/i.test(modelId)
  );
}

function catalogReasoningCapability(
  providerId: string,
  modelId: string,
  catalog: ModelCatalogEntry | undefined,
): ReasoningUiCapability | null {
  if (modelLooksOpenAiReasoning(providerId, modelId)) {
    return {
      supported: true,
      control: "effort",
      tagOnly: false,
      supportedModes: REASONING_STRENGTH_OPTIONS,
      defaultMode: "medium",
      disableSupported: true,
    };
  }
  if (catalog?.providerId === "anthropic" && catalog.supportsThinking) {
    return {
      supported: true,
      control: "budget",
      tagOnly: false,
      supportedModes: REASONING_STRENGTH_OPTIONS,
      defaultMode: "medium",
      disableSupported: true,
    };
  }
  if (modelLooksGlmReasoning(providerId, modelId)) {
    return {
      supported: true,
      control: "effort",
      tagOnly: false,
      supportedModes: REASONING_EFFORT_OPTIONS,
      defaultMode: "medium",
      disableSupported: true,
    };
  }
  if (modelLooksQwenReasoning(providerId, modelId)) {
    return {
      supported: true,
      control: "tag",
      tagOnly: true,
      supportedModes: REASONING_SWITCH_OPTIONS,
      defaultMode: "auto",
      disableSupported: true,
    };
  }
  if (catalog?.providerId === "deepseek" && catalog.supportsThinking) {
    return {
      supported: true,
      control: "switch",
      tagOnly: false,
      supportedModes: REASONING_SWITCH_OPTIONS,
      defaultMode: "auto",
      disableSupported: true,
    };
  }
  if (catalog?.providerId === "mimo" && catalog.supportsThinking) {
    return {
      supported: true,
      control: "switch",
      tagOnly: true,
      supportedModes: REASONING_SWITCH_OPTIONS,
      defaultMode: "auto",
      disableSupported: true,
    };
  }
  return null;
}

function reasoningOptionsForCapability(
  capability: ReasoningUiCapability,
): ReasoningMode[] {
  if (!capability.supported) return [];
  if (capability.supportedModes.length > 0) return capability.supportedModes;
  if (
    capability.control === "effort" ||
    capability.control === "level" ||
    capability.control === "budget"
  ) {
    return REASONING_STRENGTH_OPTIONS;
  }
  return REASONING_SWITCH_OPTIONS;
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

  const providerInfo = (providerId: string) =>
    data?.providers.find((provider) => provider.id === providerId);

  const providerRequiresBaseUrl = (providerId: string): boolean =>
    isCustomProviderId(providerId) ||
    providerInfo(providerId)?.endpointManaged === "custom";

  const sanitizeProviderOverride = (
    provider: ProviderOverride,
    providerId: string,
  ): ProviderOverride => {
    const modelCapabilities =
      provider.modelCapabilities &&
      Object.keys(provider.modelCapabilities).length > 0
        ? provider.modelCapabilities
        : undefined;
    return {
      baseUrl: providerRequiresBaseUrl(providerId)
        ? (provider.baseUrl ?? null)
        : null,
      label: provider.label ?? null,
      defaultModel: provider.defaultModel ?? null,
      enabledModels: provider.enabledModels ?? [],
      ...(modelCapabilities ? { modelCapabilities } : {}),
    };
  };

  const sanitizeRoutingForSave = (
    source: LlmRoutingConfig,
  ): LlmRoutingConfig => {
    const normalized = normalizeRouting(source);
    const providers: LlmRoutingConfig["providers"] = {};
    for (const [id, provider] of Object.entries(normalized.providers)) {
      providers[id] = sanitizeProviderOverride(provider, id);
    }
    const slots: LlmRoutingConfig["slots"] = {};
    for (const slot of CAPABILITY_SLOTS) {
      const route = normalized.slots[slot];
      if (!route?.providerId || !route.model) continue;
      slots[slot] = route;
    }
    return {
      ...normalized,
      providers,
      slots,
      contextStrategy: normalized.contextStrategy,
    };
  };

  const providerOverrideForSave = (providerId: string): ProviderOverride => {
    const existing = routingRef.current?.providers[providerId];
    return sanitizeProviderOverride(
      {
        baseUrl: providerRequiresBaseUrl(providerId)
          ? baseUrlForProvider(providerId).trim() || null
          : null,
        label: existing?.label ?? null,
        defaultModel: existing?.defaultModel ?? null,
        enabledModels: existing?.enabledModels ?? [],
        modelCapabilities: existing?.modelCapabilities,
      },
      providerId,
    );
  };

  const emptyProviderOverride = (providerId: string): ProviderOverride =>
    sanitizeProviderOverride(
      {
        baseUrl: providerRequiresBaseUrl(providerId)
          ? baseUrlForProvider(providerId).trim() || null
          : null,
        label: null,
        defaultModel: null,
        enabledModels: [],
      },
      providerId,
    );

  const modelById = (
    providerId: string,
    modelId: string,
  ): ModelCatalogEntry | undefined =>
    findModelCatalogForProvider(data?.catalog, providerId, modelId);

  const baseUrlForProvider = (providerId: string): string =>
    providerBaseUrlInputs[providerId] ??
    routing?.providers[providerId]?.baseUrl ??
    "";

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
    const prev =
      routing.providers[providerId] ?? emptyProviderOverride(providerId);
    const next = sanitizeProviderOverride({ ...prev, ...patch }, providerId);
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

  const persistRouting = async (nextRouting?: LlmRoutingConfig) => {
    const snapshot = nextRouting ?? routingRef.current;
    if (!snapshot) return;
    await llmConfigSet(sanitizeRoutingForSave(snapshot));
    setLoadError(null);
    notifyLlmConfigChanged();
  };

  const persistProviderConfig = async (providerId: string) => {
    const current = routingRef.current;
    if (!current) return false;
    if (
      providerRequiresBaseUrl(providerId) &&
      !baseUrlForProvider(providerId).trim()
    ) {
      setMessage(`${providerName(providerId)} 需配置 Base URL 后才能保存。`);
      return false;
    }
    const nextRouting: LlmRoutingConfig = sanitizeRoutingForSave({
      ...current,
      providers: {
        ...current.providers,
        [providerId]: providerOverrideForSave(providerId),
      },
    });
    applyRouting(nextRouting);
    await persistRouting(nextRouting);
    return true;
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
      providers: {
        ...routing.providers,
        [id]: sanitizeProviderOverride(entry, id),
      },
    };
    applyRouting(nextRouting);
    setData({
      ...data,
      routing: nextRouting,
      providers: [
        ...data.providers,
        {
          id,
          name: label,
          default_model: "",
          endpointManaged: "custom",
        },
      ],
    });
    void refreshKeyStatus([id]);
    setWizardOpen(false);
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
      const persisted = await persistProviderConfig(providerId);
      if (!persisted) return;
      await credentialSet(service, value);
      keyInputsRef.current[providerId] = "";
      setKeyInputTouch((n) => n + 1);
      setKeyConfigured((prev) => ({ ...prev, [providerId]: true }));
      setLoadError(null);
      setMessage(`${label} 已添加，Key 已保存到系统凭据管理器。`);
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
      setLoadError(null);
      setMessage(`${label} Key 已清除`);
      notifyLlmConfigChanged();
    } catch (err) {
      setMessage(`清除 ${label} Key 失败：${invokeErrorMessage(err)}`);
    }
  };

  const updateSlot = (
    slot: CapabilitySlot,
    patch: Partial<{
      providerId: string;
      model: string;
      thinking: boolean;
      reasoning: ReasoningSlotConfig;
    }>,
  ) => {
    if (!routing) return;
    const current = routing.slots[slot];
    if (!current && (!patch.providerId || !patch.model)) return;
    const nextRoute = { ...current, ...patch };
    const nextSlots: LlmRoutingConfig["slots"] = {
      ...routing.slots,
      [slot]: nextRoute as SlotRoute,
    };
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
      const sanitized = sanitizeRoutingForSave(routing);
      await llmConfigSet(sanitized);
      applyRouting(sanitized);
      setLoadError(null);
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
      catalog: modelById(providerId, modelId),
      registry: registryEntryForModel(providerId, modelId),
    }));
  };

  const isProviderConfiguredForRouting = (providerId: string): boolean => {
    const override = routing?.providers[providerId];
    return Boolean(
      (override && override?.enabledModels?.length) ||
      override?.defaultModel?.trim(),
    );
  };

  const modelsForSlot = (
    slot: CapabilitySlot,
    providerId: string,
  ): EnabledProviderModel[] =>
    enabledModelsForProvider(providerId).filter((model) =>
      modelSupportsSlot(model, slot),
    );

  const reasoningCapabilityForModel = (
    slot: CapabilitySlot,
    providerId: string,
    modelId: string,
  ): ReasoningUiCapability => {
    if (slot === "vision" || !providerId || !modelId) {
      return UNSUPPORTED_REASONING_CAPABILITY;
    }
    const override =
      routing?.providers[providerId]?.modelCapabilities?.[modelId] ?? null;
    if (override?.reasoningControl && override.reasoningControl !== "none") {
      return {
        supported: true,
        control: override.reasoningControl,
        tagOnly:
          override.reasoningAdapter === "open_ai_compatible_tag_stream" ||
          override.reasoningControl === "tag" ||
          override.reasoningVisibility === "content_tag" ||
          override.reasoningVisibility === "plain_content_risk",
        supportedModes:
          override.supportedModes && override.supportedModes.length > 0
            ? override.supportedModes
            : reasoningOptionsForCapability({
                supported: true,
                control: override.reasoningControl,
                tagOnly: false,
                supportedModes: [],
                defaultMode: override.defaultMode ?? "auto",
                disableSupported: override.disableSupported ?? true,
              }),
        defaultMode: override.defaultMode ?? "auto",
        disableSupported: override.disableSupported ?? true,
      };
    }
    if (
      override?.reasoningAdapter === "open_ai_compatible_tag_stream" ||
      modelLooksTagReasoningRisk(providerId, modelId)
    ) {
      return {
        supported: true,
        control: "tag",
        tagOnly: true,
        supportedModes: REASONING_SWITCH_OPTIONS,
        defaultMode: "auto",
        disableSupported: true,
      };
    }
    const catalog = modelById(providerId, modelId);
    return (
      catalogReasoningCapability(providerId, modelId, catalog) ??
      UNSUPPORTED_REASONING_CAPABILITY
    );
  };

  const reasoningOptionsForModel = (
    slot: CapabilitySlot,
    providerId: string,
    modelId: string,
  ): ReasoningMode[] =>
    reasoningOptionsForCapability(
      reasoningCapabilityForModel(slot, providerId, modelId),
    );

  const clampReasoningForModel = (
    slot: CapabilitySlot,
    providerId: string,
    modelId: string,
    current?: ReasoningSlotConfig,
  ): ReasoningSlotConfig => {
    const options = reasoningOptionsForModel(slot, providerId, modelId);
    if (options.length === 0) return { mode: "off" };
    const capability = reasoningCapabilityForModel(slot, providerId, modelId);
    const mode = current?.mode ?? capability.defaultMode;
    return {
      mode: options.includes(mode) ? mode : capability.defaultMode,
    };
  };

  const providersForSlot = (slot: CapabilitySlot): VisibleProvider[] =>
    visibleProviders.filter(
      (provider) =>
        isProviderConfiguredForRouting(provider.id) &&
        modelsForSlot(slot, provider.id).length > 0,
    );

  const modelUsageLabels = (providerId: string, modelId: string) =>
    USER_CONFIGURABLE_CAPABILITY_SLOTS.filter((slot) => {
      const route = routing?.slots[slot];
      return route?.providerId === providerId && route.model === modelId;
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

  const modelReasoningOverrideValue = (
    providerId: string,
    modelId: string,
  ): string => {
    const override =
      routing?.providers[providerId]?.modelCapabilities?.[modelId];
    if (!override?.reasoningAdapter) return "auto";
    if (override.reasoningAdapter === "none") return "none";
    if (override.reasoningAdapter === "deep_seek_reasoning_content") {
      return "reasoning_content";
    }
    if (override.reasoningAdapter === "open_ai_compatible_tag_stream") {
      return "tag_stream";
    }
    if (override.reasoningAdapter === "qwen_chat_template") {
      return "tag_template";
    }
    if (override.reasoningControl === "effort") return "native_effort";
    if (override.reasoningControl === "budget") return "native_budget";
    if (override.reasoningControl === "level") return "native_level";
    return "native_switch";
  };

  const updateModelReasoningOverride = (
    providerId: string,
    modelId: string,
    value: string,
  ) => {
    if (!routing) return;
    const provider = routing.providers[providerId];
    if (!provider) return;
    const nextCapabilities = { ...(provider.modelCapabilities ?? {}) };
    if (value === "auto") {
      delete nextCapabilities[modelId];
    } else {
      nextCapabilities[modelId] =
        value === "none"
          ? {
              reasoningAdapter: "none",
              reasoningControl: "none",
              reasoningVisibility: "hidden_channel",
              supportedModes: ["off"],
              defaultMode: "off",
              disableSupported: true,
              userVerifiedAt: new Date().toISOString(),
              probeVerifiedAt: null,
            }
          : value === "reasoning_content"
            ? {
                reasoningAdapter: "deep_seek_reasoning_content",
                reasoningControl: "switch",
                reasoningVisibility: "hidden_channel",
                supportedModes: REASONING_SWITCH_OPTIONS,
                defaultMode: "auto",
                disableSupported: true,
                userVerifiedAt: new Date().toISOString(),
                probeVerifiedAt: null,
              }
            : value === "tag_stream"
              ? {
                  reasoningAdapter: "open_ai_compatible_tag_stream",
                  reasoningControl: "tag",
                  reasoningVisibility: "plain_content_risk",
                  supportedModes: REASONING_SWITCH_OPTIONS,
                  defaultMode: "auto",
                  disableSupported: true,
                  userVerifiedAt: new Date().toISOString(),
                  probeVerifiedAt: null,
                }
              : value === "tag_template"
                ? {
                    reasoningAdapter: "qwen_chat_template",
                    reasoningControl: "tag",
                    reasoningVisibility: "content_tag",
                    supportedModes: REASONING_SWITCH_OPTIONS,
                    defaultMode: "auto",
                    disableSupported: true,
                    userVerifiedAt: new Date().toISOString(),
                    probeVerifiedAt: null,
                  }
                : value === "native_effort"
                  ? {
                      reasoningAdapter: "glm_thinking",
                      reasoningControl: "effort",
                      reasoningVisibility: "hidden_channel",
                      supportedModes: REASONING_EFFORT_OPTIONS,
                      defaultMode: "medium",
                      disableSupported: true,
                      userVerifiedAt: new Date().toISOString(),
                      probeVerifiedAt: null,
                    }
                  : value === "native_budget"
                    ? {
                        reasoningAdapter: "anthropic_extended_thinking",
                        reasoningControl: "budget",
                        reasoningVisibility: "hidden_channel",
                        supportedModes: REASONING_STRENGTH_OPTIONS,
                        defaultMode: "medium",
                        disableSupported: true,
                        userVerifiedAt: new Date().toISOString(),
                        probeVerifiedAt: null,
                      }
                    : value === "native_level"
                      ? {
                          reasoningAdapter: "gemini_thinking_config",
                          reasoningControl: "level",
                          reasoningVisibility: "hidden_channel",
                          supportedModes: REASONING_EFFORT_OPTIONS,
                          defaultMode: "medium",
                          disableSupported: true,
                          userVerifiedAt: new Date().toISOString(),
                          probeVerifiedAt: null,
                        }
                      : {
                          reasoningAdapter: "provider_specific_static",
                          reasoningControl: "switch",
                          reasoningVisibility: "hidden_channel",
                          supportedModes: REASONING_SWITCH_OPTIONS,
                          defaultMode: "auto",
                          disableSupported: true,
                          userVerifiedAt: new Date().toISOString(),
                          probeVerifiedAt: null,
                        };
    }
    updateProviderOverride(providerId, {
      modelCapabilities:
        Object.keys(nextCapabilities).length > 0 ? nextCapabilities : undefined,
    });
  };

  const visibleProviders = (() => {
    if (!routing || !data) return [];
    const configuredProviderIds = Object.keys(routing.providers);
    const providers = configuredProviderIds.map((providerId) => {
      const override = routing.providers[providerId];
      const usedSlots = USER_CONFIGURABLE_CAPABILITY_SLOTS.filter((slot) => {
        const route = routing.slots[slot];
        return route?.providerId === providerId;
      }).map((slot) => SLOT_META[slot].label);
      return {
        id: providerId,
        name: providerName(providerId),
        enabledModels: enabledModelIdsForProvider(providerId),
        usedSlots,
        configured: Boolean(override),
        custom: isCustomProviderId(providerId),
        endpointManaged: providerInfo(providerId)?.endpointManaged ?? "custom",
      };
    });

    return providers.sort((a, b) => {
      const score = (provider: VisibleProvider) =>
        provider.usedSlots.length > 0 ? 0 : provider.configured ? 1 : 2;
      return score(a) - score(b) || a.name.localeCompare(b.name);
    });
  })();

  const testProvider = async (provider: VisibleProvider) => {
    if (!(await persistProviderConfig(provider.id))) return;
    setTesting(provider.id);
    setProviderResults((prev) => {
      const next = { ...prev };
      delete next[provider.id];
      return next;
    });
    try {
      const result = await llmConfigTestProvider(provider.id);
      setLoadError(null);
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

  const deleteProvider = async (provider: VisibleProvider) => {
    if (provider.usedSlots.length > 0) {
      setMessage(
        provider.name +
          " is used by " +
          provider.usedSlots.join(" / ") +
          "; adjust routing before deleting it.",
      );
      return;
    }
    const confirmed = confirm(
      "Delete " +
        provider.name +
        "? This removes its provider configuration, enabled models, model validation rows, and credential marker. Built-in providers can be added again later.",
    );
    if (!confirmed || !data) return;
    setMessage(null);
    try {
      const nextRouting = normalizeRouting(
        await llmConfigDeleteProvider(provider.id),
      );
      applyRouting(nextRouting);
      setLoadError(null);
      setData({
        ...data,
        routing: nextRouting,
        providers: isCustomProviderId(provider.id)
          ? data.providers.filter((item) => item.id !== provider.id)
          : data.providers,
        registry: data.registry.filter(
          (entry) => entry.providerId !== provider.id,
        ),
      });
      setProviderBaseUrlInputs((prev) => {
        const next = { ...prev };
        delete next[provider.id];
        return next;
      });
      setNewModelInputs((prev) => {
        const next = { ...prev };
        delete next[provider.id];
        return next;
      });
      setKeyConfigured((prev) => {
        const next = { ...prev };
        delete next[provider.id];
        return next;
      });
      setProviderResults((prev) => {
        const next = { ...prev };
        delete next[provider.id];
        return next;
      });
      setTestResults((prev) => {
        const next: typeof prev = {};
        for (const [key, value] of Object.entries(prev)) {
          if (!key.startsWith(provider.id + ":")) next[key] = value;
        }
        return next;
      });
      setMessage(provider.name + " deleted");
      notifyLlmConfigChanged();
    } catch (err) {
      setMessage(
        "Delete " + provider.name + " failed: " + invokeErrorMessage(err),
      );
    }
  };
  const refreshProviderModels = async (provider: VisibleProvider) => {
    if (!(await persistProviderConfig(provider.id))) return;
    setRefreshingProvider(provider.id);
    try {
      const result = await llmModelRegistryRefresh(provider.id);
      setLoadError(null);
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
  ) => {
    const key = `${provider.id}:${model.id}`;
    if (!(await persistProviderConfig(provider.id))) {
      setTestResults((prev) => ({
        ...prev,
        [key]: {
          ok: false,
          message: "供应商配置未保存",
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
      const text = await llmModelValidate(provider.id, model.id, "text");
      if (!text.ok) {
        setTestResults((prev) => ({
          ...prev,
          [key]: { ok: false, message: "文本不可用" },
        }));
        return;
      }

      const vision = await llmModelValidate(provider.id, model.id, "vision");
      const message = vision.ok
        ? "文本可用 · 视觉可用"
        : "文本可用 · 视觉不支持";
      setLoadError(null);
      setTestResults((prev) => ({
        ...prev,
        [key]: { ok: true, message },
      }));
      await load({ preserveRouting: true });
    } catch (err) {
      console.warn("[LlmRouting] model validation failed:", err);
      setTestResults((prev) => ({
        ...prev,
        [key]: { ok: false, message: "验证失败" },
      }));
    } finally {
      setTesting(null);
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
            onBaseUrl={(id, url) => updateProviderBaseUrl(id, url)}
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
              const override = routing.providers[provider.id];
              const providerModels = enabledModelsForProvider(provider.id);
              const providerResult = providerResults[provider.id];
              const requiresBaseUrl = providerRequiresBaseUrl(provider.id);
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
                      {providerModels.length} 个已启用模型
                      {provider.usedSlots.length > 0
                        ? ` · 用于 ${provider.usedSlots.join(" / ")}`
                        : ""}
                    </p>
                    {isCustomProviderId(provider.id) ? (
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
                    {requiresBaseUrl ? (
                      <Input
                        className="h-8 text-xs"
                        placeholder="自定义端点 Base URL"
                        value={baseUrlForProvider(provider.id)}
                        onChange={(event) =>
                          updateProviderBaseUrl(provider.id, event.target.value)
                        }
                      />
                    ) : (
                      <p className="rounded-md border border-border/45 bg-background/45 px-3 py-2 text-[11px] text-muted-foreground">
                        内置供应商使用系统默认端点
                      </p>
                    )}
                    <div className="flex flex-wrap items-center gap-2">
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
                    </div>
                    <p className="text-[11px] text-muted-foreground">
                      {keyConfigured[provider.id]
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
                        {testing === provider.id ? "检查中…" : "检查端点"}
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
                      {provider.configured ? (
                        <Button
                          type="button"
                          size="sm"
                          variant="ghost"
                          className="h-7 text-xs text-destructive"
                          onClick={() => void deleteProvider(provider)}
                        >
                          Delete
                        </Button>
                      ) : null}
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
                        const modelTesting = testing === key;
                        const summary = modelCapabilitySummary(model, result);
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
                                <Select
                                  value={modelReasoningOverrideValue(
                                    provider.id,
                                    model.id,
                                  )}
                                  onValueChange={(value) =>
                                    updateModelReasoningOverride(
                                      provider.id,
                                      model.id,
                                      value,
                                    )
                                  }
                                >
                                  <SelectTrigger
                                    aria-label={`${model.id} 思考能力覆盖`}
                                    className="h-7 w-28 text-xs"
                                  >
                                    <SelectValue placeholder="思考能力" />
                                  </SelectTrigger>
                                  <SelectContent>
                                    <SelectItem value="auto">
                                      自动识别
                                    </SelectItem>
                                    <SelectItem value="none">
                                      不支持思考
                                    </SelectItem>
                                    <SelectItem value="native_switch">
                                      开关思考
                                    </SelectItem>
                                    <SelectItem value="native_effort">
                                      强度思考
                                    </SelectItem>
                                    <SelectItem value="native_budget">
                                      预算思考
                                    </SelectItem>
                                    <SelectItem value="native_level">
                                      等级思考
                                    </SelectItem>
                                    <SelectItem value="reasoning_content">
                                      reasoning_content
                                    </SelectItem>
                                    <SelectItem value="tag_template">
                                      tag 模板
                                    </SelectItem>
                                    <SelectItem value="tag_stream">
                                      标签隔离
                                    </SelectItem>
                                  </SelectContent>
                                </Select>
                                <Button
                                  type="button"
                                  size="sm"
                                  variant="secondary"
                                  className="h-7 text-xs"
                                  disabled={modelTesting}
                                  onClick={() =>
                                    void validateProviderModel(provider, model)
                                  }
                                >
                                  {modelTesting ? "验证中…" : "验证模型"}
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
                            <div className="mt-2 flex flex-wrap items-center gap-2">
                              <span
                                className={
                                  result?.ok === false
                                    ? "text-[11px] text-destructive"
                                    : "text-[11px] text-muted-foreground"
                                }
                              >
                                {summary}
                              </span>
                              <ModelDebugDetails model={model.catalog} />
                            </div>
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

      <section className="space-y-2" data-section="llm-capability-routing">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="text-xs font-medium text-muted-foreground">
            能力槽模型路由
          </p>
        </div>

        <div className="space-y-2">
          {USER_CONFIGURABLE_CAPABILITY_SLOTS.map((slot) => {
            const route = routing.slots[slot];
            const routeProviderOptions = providersForSlot(slot);
            const routeProviderIds = routeProviderOptions.map(
              (provider) => provider.id,
            );
            const providerId =
              route && routeProviderIds.includes(route.providerId)
                ? route.providerId
                : "";
            const routeProviderInvalid =
              Boolean(route?.providerId) && route?.providerId !== providerId;
            const models = providerId ? modelsForSlot(slot, providerId) : [];
            const modelIds = models.map((model) => model.id);
            const selectedModel =
              route && modelIds.includes(route.model) ? route.model : "";
            const routeModelInvalid =
              Boolean(route?.model) && route?.model !== selectedModel;
            const reasoningOptions =
              slot !== "vision" && providerId && selectedModel
                ? reasoningOptionsForModel(slot, providerId, selectedModel)
                : [];
            const reasoningCapability =
              slot !== "vision" && providerId && selectedModel
                ? reasoningCapabilityForModel(slot, providerId, selectedModel)
                : UNSUPPORTED_REASONING_CAPABILITY;
            const selectedReasoning = clampReasoningForModel(
              slot,
              providerId,
              selectedModel,
              normalizeReasoningSlot(route),
            ).mode;
            return (
              <div
                key={slot}
                className="grid gap-2 rounded-md border border-border/50 bg-background/60 p-2 xl:grid-cols-[minmax(8rem,0.85fr)_1fr_1.2fr_0.8fr_1fr]"
              >
                <div className="min-w-0 self-center">
                  <p className="text-xs font-medium text-foreground">
                    {SLOT_META[slot].label}
                  </p>
                  <p className="mt-0.5 text-[11px] text-muted-foreground">
                    {SLOT_META[slot].detail}
                  </p>
                </div>
                {routeProviderOptions.length === 0 ? (
                  <Input
                    className="h-8 text-xs"
                    placeholder={
                      slot === "vision" ? "无可用视觉模型" : "无可用供应商"
                    }
                    value=""
                    disabled
                  />
                ) : (
                  <Select
                    value={providerId}
                    onValueChange={(value) =>
                      updateSlot(slot, {
                        providerId: value,
                        model: modelsForSlot(slot, value)[0]?.id ?? "",
                        reasoning: clampReasoningForModel(
                          slot,
                          value,
                          modelsForSlot(slot, value)[0]?.id ?? "",
                        ),
                      })
                    }
                  >
                    <SelectTrigger className="h-8 text-xs">
                      <SelectValue placeholder="选择供应商" />
                    </SelectTrigger>
                    <SelectContent>
                      {routeProviderOptions.map((provider) => (
                        <SelectItem key={provider.id} value={provider.id}>
                          {provider.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                )}
                {models.length === 0 ? (
                  <Input
                    className="h-8 text-xs"
                    placeholder={
                      slot === "vision"
                        ? "无可用视觉模型"
                        : providerId
                          ? "先在供应商配置中添加模型"
                          : "先选择供应商"
                    }
                    value=""
                    disabled
                  />
                ) : (
                  <Select
                    value={selectedModel}
                    onValueChange={(value) =>
                      updateSlot(slot, {
                        providerId,
                        model: value,
                        reasoning: clampReasoningForModel(
                          slot,
                          providerId,
                          value,
                          normalizeReasoningSlot(route),
                        ),
                      })
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
                {slot === "vision" ? (
                  <div className="self-center text-[11px] text-muted-foreground" />
                ) : reasoningOptions.length === 0 ? (
                  <Input
                    aria-label={`${SLOT_META[slot].label} 思考模式`}
                    className="h-8 text-xs"
                    value="不支持"
                    disabled
                    readOnly
                  />
                ) : (
                  <Select
                    value={selectedReasoning}
                    onValueChange={(value) =>
                      updateSlot(slot, {
                        providerId,
                        model: selectedModel,
                        reasoning: { mode: value as ReasoningMode },
                      })
                    }
                  >
                    <SelectTrigger
                      aria-label={`${SLOT_META[slot].label} 思考模式`}
                      className="h-8 text-xs"
                    >
                      <SelectValue placeholder="思考模式" />
                    </SelectTrigger>
                    <SelectContent>
                      {reasoningOptions.map((mode) => (
                        <SelectItem key={mode} value={mode}>
                          {REASONING_LABELS[mode]}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                )}
                <div className="self-center text-[11px] text-muted-foreground">
                  {routeProviderInvalid || routeModelInvalid
                    ? "当前路由不可用，请重新选择"
                    : reasoningCapability.tagOnly
                      ? "可能以正文标签形式返回思考"
                      : ""}
                </div>
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
  const selectedProvider = providers.find(
    (provider) => provider.id === providerId,
  );
  const custom =
    isCustomProviderId(providerId) ||
    selectedProvider?.endpointManaged === "custom";

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
            placeholder="自定义端点 Base URL"
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

function ModelDebugDetails({
  model,
}: {
  model: ModelCatalogEntry | undefined;
}) {
  if (!model) {
    return (
      <details className="text-[10px] text-muted-foreground">
        <summary className="cursor-pointer select-none">详情</summary>
        <span className="mt-1 inline-block rounded border border-border/50 px-1.5 py-0.5">
          manual model
        </span>
      </details>
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
    <details className="text-[10px] text-muted-foreground">
      <summary className="cursor-pointer select-none">详情</summary>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        {tags.map((tag) => (
          <span
            key={tag}
            className="rounded border border-border/50 px-1.5 py-0.5"
          >
            {tag}
          </span>
        ))}
      </div>
    </details>
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function normalizeRouting(raw: LlmRoutingConfig | undefined): LlmRoutingConfig {
  const rawRecord: Record<string, unknown> = isRecord(raw)
    ? raw
    : (DEFAULT_LLM_ROUTING as unknown as Record<string, unknown>);
  const rawProviders = isRecord(rawRecord.providers) ? rawRecord.providers : {};
  const providers: LlmRoutingConfig["providers"] = {};
  for (const [id, provider] of Object.entries(rawProviders)) {
    const row = (isRecord(provider)
      ? provider
      : {}) as unknown as ProviderOverride & {
      base_url?: string | null;
      default_model?: string | null;
      enabled_models?: string[] | null;
      model_capabilities?: ProviderOverride["modelCapabilities"] | null;
      modelCapabilities?: ProviderOverride["modelCapabilities"] | null;
    };
    const rawModelCapabilities =
      row.modelCapabilities ?? row.model_capabilities;
    const modelCapabilities = isRecord(rawModelCapabilities)
      ? (rawModelCapabilities as ProviderOverride["modelCapabilities"])
      : undefined;
    providers[id] = {
      baseUrl: row.baseUrl ?? row.base_url ?? null,
      label: row.label ?? null,
      defaultModel: row.defaultModel ?? row.default_model ?? null,
      enabledModels: Array.isArray(row.enabledModels)
        ? row.enabledModels
        : Array.isArray(row.enabled_models)
          ? row.enabled_models
          : [],
      ...(modelCapabilities && Object.keys(modelCapabilities).length > 0
        ? { modelCapabilities }
        : {}),
    };
  }

  const slots: LlmRoutingConfig["slots"] = {};
  const legacyScenes = isRecord(rawRecord.scenes) ? rawRecord.scenes : {};
  const legacySceneToSlot: Partial<Record<CapabilitySlot, string>> = {
    fast: "knowledge_lookup",
    writer: "drafting_assist",
    reasoner: "research_synthesis",
    long_context: "exemplar_learning",
    agent_tools: "knowledge_lookup",
  };
  for (const [slot, scene] of Object.entries(legacySceneToSlot)) {
    const route = legacyScenes[scene];
    if (!isRecord(route)) continue;
    const row = route as unknown as SlotRoute & { provider_id?: string };
    const providerId = row.providerId ?? row.provider_id;
    if (!providerId || !row.model) continue;
    slots[slot as CapabilitySlot] = {
      providerId,
      model: normalizePersistedModelId(row.model),
      thinking: row.thinking ?? false,
      reasoning: normalizeReasoningSlot(row),
    };
  }
  const rawSlots = isRecord(rawRecord.slots) ? rawRecord.slots : {};
  for (const slot of CAPABILITY_SLOTS) {
    const route = rawSlots[slot];
    if (!isRecord(route)) continue;
    const row = route as unknown as SlotRoute & { provider_id?: string };
    const providerId = row.providerId ?? row.provider_id;
    if (!providerId || !row.model) continue;
    slots[slot] = {
      providerId,
      model: normalizePersistedModelId(row.model),
      thinking: row.thinking ?? false,
      reasoning: normalizeReasoningSlot(row),
    };
  }

  const contextStrategy = isRecord(rawRecord.contextStrategy)
    ? (rawRecord.contextStrategy as LlmRoutingConfig["contextStrategy"])
    : DEFAULT_LLM_ROUTING.contextStrategy;

  return {
    version: typeof rawRecord.version === "number" ? rawRecord.version : 1,
    schemaVersion:
      typeof rawRecord.schemaVersion === "number" ? rawRecord.schemaVersion : 4,
    providers,
    slots,
    contextStrategy,
  };
}

function normalizePersistedModelId(model: string): string {
  return model === "mimo-vl-7b-experimental" ? "MiMo-V2.5-Pro" : model;
}
