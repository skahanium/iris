import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { isTauri } from "@tauri-apps/api/core";

import { invokeErrorMessage, llmCredentialService } from "@/lib/credentials";
import {
  credentialDelete,
  credentialStatus,
  credentialSet,
  llmConfigDeleteProvider,
  llmConfigGet,
  llmConfigSet,
  llmConfigTestProvider,
  llmModelRegistryRefresh,
  llmModelValidate,
} from "@/lib/ipc";
import { notifyLlmConfigChanged } from "@/lib/llm-events";
import {
  DEFAULT_LLM_ROUTING,
  isCustomProviderId,
  type LlmConfigGetResponse,
  type LlmRoutingConfig,
  type ModelRegistryEntry,
  type ModelCatalogEntry,
  type ProviderOverride,
} from "@/types/llm";
import { LlmProviderDetail } from "./LlmProviderDetail";
import { LlmProviderListCard } from "./LlmProviderListCard";
import type {
  LlmEnabledProviderModel,
  LlmVisibleProvider,
} from "./llmProviderTypes";
import { AddModelWizard } from "./AddModelWizard";
import { LlmModelPoolSection } from "./LlmModelPoolSection";
import {
  FALLBACK_PROVIDERS,
  REASONING_SWITCH_OPTIONS,
  UNSUPPORTED_REASONING_CAPABILITY,
  catalogReasoningCapability,
  findModelCatalogForProvider,
  modelCapabilitySummary,
  nextCustomProviderId,
  normalizeCandidateOrder,
  parseModelIds,
  reasoningCapabilitySummary,
  reasoningOptionsForCapability,
  uniqueModelIds,
  type ReasoningUiCapability,
} from "./llmRoutingModelHelpers";

import type { ManagementProviderChrome } from "./managementProviderChrome";

interface LlmRoutingSectionProps {
  open: boolean;
  selectedProviderId: string | null;
  onSelectedProviderIdChange: (providerId: string | null) => void;
  onProviderChromeChange?: (chrome: ManagementProviderChrome | null) => void;
}

type VisibleProvider = LlmVisibleProvider;

type EnabledProviderModel = LlmEnabledProviderModel;

export function LlmRoutingSection({
  open,
  selectedProviderId,
  onSelectedProviderIdChange,
  onProviderChromeChange,
}: LlmRoutingSectionProps) {
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
            configured[id] = (
              await credentialStatus(llmCredentialService(id))
            ).configured;
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
    return {
      ...normalized,
      providers,
      schemaVersion: 6,
      candidateOrder: normalizeCandidateOrder(
        providers,
        normalized.candidateOrder.length > 0
          ? normalized.candidateOrder
          : normalized.defaultModel
            ? [normalized.defaultModel]
            : [],
      ),
      defaultModel: null,
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
          requiresApiKey: true,
        },
      ],
    });
    void refreshKeyStatus([id]);
    setWizardOpen(false);
    onSelectedProviderIdChange(id);
    return id;
  };

  const saveProviderKeyValue = async (
    providerId: string,
    value: string,
    options: { silent?: boolean } = {},
  ) => {
    const trimmed = value.trim();
    if (!trimmed) return false;
    const label = providerName(providerId);

    keyStatusEpochRef.current += 1;
    const persisted = await persistProviderConfig(providerId);
    if (!persisted) return false;
    const status = await credentialSet(
      llmCredentialService(providerId),
      trimmed,
    );
    setKeyInputTouch((n) => n + 1);
    setKeyConfigured((prev) => ({
      ...prev,
      [providerId]: status.configured,
    }));
    setLoadError(null);
    if (!options.silent) {
      setMessage(`${label} 已添加，Key 已保存到本地加密凭据。`);
    }
    notifyLlmConfigChanged();
    return status.configured;
  };

  const saveKey = async (providerId: string) => {
    const value = keyInputsRef.current[providerId]?.trim();
    if (!value) return;
    const label = providerName(providerId);

    setKeySaving(providerId);
    setMessage(null);
    try {
      await saveProviderKeyValue(providerId, value);
      if (wizardOpen) {
        setWizardOpen(false);
        onSelectedProviderIdChange(providerId);
      }
    } catch (err) {
      setMessage(`保存 ${label} Key 失败：${invokeErrorMessage(err)}`);
    } finally {
      setKeySaving(null);
    }
  };

  const ensureProviderKeySavedForProbe = async (
    providerId: string,
    apiKeyOverride: string | undefined,
  ) => {
    const typedKey = apiKeyOverride?.trim();
    if (typedKey) {
      try {
        return await saveProviderKeyValue(providerId, typedKey, {
          silent: true,
        });
      } catch (err) {
        setMessage(
          `保存 ${providerName(providerId)} Key 失败：${invokeErrorMessage(err)}`,
        );
        return false;
      }
    }
    return persistProviderConfig(providerId);
  };

  const clearKey = async (providerId: string) => {
    const label = providerName(providerId);
    keyStatusEpochRef.current += 1;
    try {
      const status = await credentialDelete(llmCredentialService(providerId));
      setKeyConfigured((prev) => ({
        ...prev,
        [providerId]: status.configured,
      }));
      setLoadError(null);
      setMessage(`${label} Key 已清除`);
      notifyLlmConfigChanged();
    } catch (err) {
      setMessage(`清除 ${label} Key 失败：${invokeErrorMessage(err)}`);
    }
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
      setMessage("模型池设置已保存");
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

  const reasoningCapabilityForModel = (
    providerId: string,
    modelId: string,
  ): ReasoningUiCapability => {
    if (!providerId || !modelId) {
      return UNSUPPORTED_REASONING_CAPABILITY;
    }
    const override =
      routing?.providers[providerId]?.modelCapabilities?.[modelId] ?? null;
    if (
      override?.reasoningAdapter === "none" ||
      override?.reasoningControl === "none"
    ) {
      return {
        supported: false,
        control: "none",
        tagOnly: false,
        supportedModes: ["off"],
        defaultMode: "off",
        disableSupported: true,
        source: override.userVerifiedAt ? "user" : "probe",
      };
    }
    if (override?.reasoningControl) {
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
                source: override.userVerifiedAt ? "user" : "probe",
              }),
        defaultMode: override.defaultMode ?? "auto",
        disableSupported: override.disableSupported ?? true,
        source: override.userVerifiedAt ? "user" : "probe",
      };
    }
    if (override?.reasoningAdapter === "open_ai_compatible_tag_stream") {
      return {
        supported: true,
        control: "tag",
        tagOnly: true,
        supportedModes: REASONING_SWITCH_OPTIONS,
        defaultMode: "auto",
        disableSupported: true,
        source: override?.probeVerifiedAt ? "probe" : "catalog",
      };
    }
    const catalog = modelById(providerId, modelId);
    return (
      catalogReasoningCapability(providerId, modelId, catalog) ??
      UNSUPPORTED_REASONING_CAPABILITY
    );
  };

  const addProviderModel = (providerId: string) => {
    if (!routing) return;
    const additions = parseModelIds(newModelInputs[providerId] ?? "");
    if (additions.length === 0) return;
    const nextEnabled = uniqueModelIds([
      ...enabledModelIdsForProvider(providerId),
      ...additions,
    ]);
    const nextProviders = {
      ...routing.providers,
      [providerId]: {
        ...(routing.providers[providerId] ?? emptyProviderOverride(providerId)),
        enabledModels: nextEnabled,
      },
    };
    applyRouting({
      ...routing,
      providers: nextProviders,
      candidateOrder: normalizeCandidateOrder(nextProviders, [
        ...routing.candidateOrder,
        ...additions.map((modelId) => ({ providerId, modelId })),
      ]),
    });
    setNewModelInputs((prev) => ({ ...prev, [providerId]: "" }));
  };

  const removeProviderModel = (providerId: string, modelId: string) => {
    if (!routing) return;
    const nextEnabled = enabledModelIdsForProvider(providerId).filter(
      (id) => id !== modelId,
    );
    const nextProviders = {
      ...routing.providers,
      [providerId]: {
        ...(routing.providers[providerId] ?? emptyProviderOverride(providerId)),
        enabledModels: nextEnabled,
      },
    };
    applyRouting({
      ...routing,
      providers: nextProviders,
      candidateOrder: routing.candidateOrder.filter(
        (candidate) =>
          candidate.providerId !== providerId || candidate.modelId !== modelId,
      ),
    });
  };

  // providerInfo / enabledModelIdsForProvider close over data+routing; providerName is stable.
  const visibleProviders = useMemo(() => {
    if (!routing || !data) return [];
    const configuredProviderIds = Object.keys(routing.providers);
    const providers = configuredProviderIds.map((providerId) => {
      const override = routing.providers[providerId];
      return {
        id: providerId,
        name: providerName(providerId),
        enabledModels: enabledModelIdsForProvider(providerId),
        configured: Boolean(override),
        custom: isCustomProviderId(providerId),
        endpointManaged: providerInfo(providerId)?.endpointManaged ?? "custom",
        requiresApiKey: providerInfo(providerId)?.requiresApiKey ?? true,
      };
    });

    return providers.sort((a, b) => a.name.localeCompare(b.name));
    // eslint-disable-next-line react-hooks/exhaustive-deps -- derived from data, routing, providerName only
  }, [data, providerName, routing]);

  useEffect(() => {
    if (!selectedProviderId || visibleProviders.length === 0) return;
    if (
      visibleProviders.some((provider) => provider.id === selectedProviderId)
    ) {
      return;
    }
    onSelectedProviderIdChange(null);
  }, [onSelectedProviderIdChange, selectedProviderId, visibleProviders]);

  const providerChromePayload = useMemo((): ManagementProviderChrome | null => {
    if (!selectedProviderId) return null;
    const provider = visibleProviders.find(
      (item) => item.id === selectedProviderId,
    );
    if (!provider) return null;
    return {
      label: provider.name,
      detail: `${provider.enabledModels.length} 个已启用模型`,
    };
  }, [selectedProviderId, visibleProviders]);

  useEffect(() => {
    if (!onProviderChromeChange || !open || !providerChromePayload) {
      return;
    }
    onProviderChromeChange(providerChromePayload);
  }, [onProviderChromeChange, open, providerChromePayload]);

  useEffect(() => {
    if (!onProviderChromeChange) return;
    return () => onProviderChromeChange(null);
  }, [onProviderChromeChange]);

  const routingForOrder = routing ?? DEFAULT_LLM_ROUTING;
  const orderedModelReferences = normalizeCandidateOrder(
    routingForOrder.providers,
    routingForOrder.candidateOrder,
  ).map((candidate) => ({
    ...candidate,
    label: `${providerName(candidate.providerId)} · ${candidate.modelId}`,
  }));

  const moveCandidate = (index: number, direction: -1 | 1) => {
    if (!routing) return;
    const target = index + direction;
    if (target < 0 || target >= orderedModelReferences.length) return;
    const candidateOrder = [...orderedModelReferences];
    const source = candidateOrder[index];
    const destination = candidateOrder[target];
    if (!source || !destination) return;
    candidateOrder[index] = destination;
    candidateOrder[target] = source;
    applyRouting({
      ...routing,
      candidateOrder: candidateOrder.map(({ providerId, modelId }) => ({
        providerId,
        modelId,
      })),
    });
  };

  const testProvider = async (provider: VisibleProvider) => {
    const apiKeyOverride = keyInputsRef.current[provider.id]?.trim();
    if (!(await ensureProviderKeySavedForProbe(provider.id, apiKeyOverride))) {
      return;
    }
    setTesting(provider.id);
    setProviderResults((prev) => {
      const next = { ...prev };
      delete next[provider.id];
      return next;
    });
    try {
      const result = await llmConfigTestProvider(provider.id, apiKeyOverride);
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
    if (
      routing?.candidateOrder.some(
        (candidate) => candidate.providerId === provider.id,
      )
    ) {
      setMessage(`${provider.name} 仍在主备模型池中，请先移除其模型。`);
      return;
    }
    const confirmed = confirm(
      "Delete " +
        provider.name +
        "? This removes its provider configuration, enabled models, and model validation rows. The stored API Key is kept unless you clear it separately.",
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
        if (!isCustomProviderId(provider.id)) return prev;
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
    const apiKeyOverride = keyInputsRef.current[provider.id]?.trim();
    if (!(await ensureProviderKeySavedForProbe(provider.id, apiKeyOverride))) {
      return;
    }
    setRefreshingProvider(provider.id);
    try {
      const result = await llmModelRegistryRefresh(provider.id, apiKeyOverride);
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
    const apiKeyOverride = keyInputsRef.current[provider.id]?.trim();
    if (!(await ensureProviderKeySavedForProbe(provider.id, apiKeyOverride))) {
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
      const text = await llmModelValidate(
        provider.id,
        model.id,
        "text",
        apiKeyOverride,
      );
      if (!text.ok) {
        setTestResults((prev) => ({
          ...prev,
          [key]: { ok: false, message: "文本不可用" },
        }));
        return;
      }

      const vision = await llmModelValidate(
        provider.id,
        model.id,
        "vision",
        apiKeyOverride,
      );
      const reasoningFromValidation = text.message.includes("推理：")
        ? text.message.slice(text.message.indexOf("推理："))
        : reasoningCapabilitySummary(
            reasoningCapabilityForModel(provider.id, model.id),
          );
      const message = vision.ok
        ? `文本可用 · 视觉可用 · ${reasoningFromValidation}`
        : `文本可用 · 视觉不支持 · ${reasoningFromValidation}`;
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
      {loadError ? (
        <p className="text-xs text-warning">未能从后端读取配置：{loadError}</p>
      ) : null}
      {keysLoading ? (
        <p className="text-[10px] text-muted-foreground">正在检查已配置凭据…</p>
      ) : null}

      {!selectedProviderId ? (
        <p className="text-xs text-muted-foreground">
          供应商只保存 API
          与端点；模型由你手动填写，未添加模型时不会激活或展示任何模型。
        </p>
      ) : null}

      <section className="space-y-2" data-section="llm-providers">
        {!selectedProviderId ? (
          <>
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
                  const providerModels = enabledModelsForProvider(provider.id);
                  return (
                    <LlmProviderListCard
                      key={provider.id}
                      providerId={provider.id}
                      providerName={provider.name}
                      providerModels={providerModels}
                      keyConfigured={Boolean(keyConfigured[provider.id])}
                      onSelect={() => onSelectedProviderIdChange(provider.id)}
                    />
                  );
                })}
              </div>
            )}
          </>
        ) : null}

        {selectedProviderId
          ? (() => {
              const provider = visibleProviders.find(
                (item) => item.id === selectedProviderId,
              );
              if (!provider) {
                return (
                  <div className="space-y-2 rounded-md border border-border/55 bg-background/60 p-3 text-xs text-muted-foreground">
                    <p>找不到该供应商，可能已被删除。</p>
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      className="h-7"
                      onClick={() => onSelectedProviderIdChange(null)}
                    >
                      返回列表
                    </Button>
                  </div>
                );
              }
              const override = routing.providers[provider.id];
              const providerModels = enabledModelsForProvider(provider.id);
              const providerResult = providerResults[provider.id];
              const requiresBaseUrl = providerRequiresBaseUrl(provider.id);
              return (
                <LlmProviderDetail
                  provider={provider}
                  override={override}
                  providerModels={providerModels}
                  providerResult={providerResult}
                  requiresBaseUrl={requiresBaseUrl}
                  baseUrl={baseUrlForProvider(provider.id)}
                  keyInput={keyInputsRef.current?.[provider.id] ?? ""}
                  keyConfigured={Boolean(keyConfigured[provider.id])}
                  keySaving={keySaving === provider.id}
                  testing={testing}
                  refreshingProvider={refreshingProvider}
                  newModelInput={newModelInputs[provider.id] ?? ""}
                  testResults={testResults}
                  modelSummary={modelCapabilitySummary}
                  reasoningSummaryForModel={(modelId) =>
                    reasoningCapabilitySummary(
                      reasoningCapabilityForModel(provider.id, modelId),
                    )
                  }
                  onKeyInput={(value) => {
                    keyInputsRef.current[provider.id] = value;
                    setKeyInputTouch((n) => n + 1);
                  }}
                  onSaveKey={() => void saveKey(provider.id)}
                  onClearKey={() => void clearKey(provider.id)}
                  onTestProvider={() => void testProvider(provider)}
                  onRefreshModels={() => void refreshProviderModels(provider)}
                  onDeleteProvider={() => void deleteProvider(provider)}
                  onBaseUrlChange={(url) =>
                    updateProviderBaseUrl(provider.id, url)
                  }
                  onLabelChange={(label) =>
                    updateProviderOverride(provider.id, {
                      label: label || null,
                    })
                  }
                  onNewModelInputChange={(value) =>
                    setNewModelInputs((prev) => ({
                      ...prev,
                      [provider.id]: value,
                    }))
                  }
                  onAddModel={() => addProviderModel(provider.id)}
                  onValidateModel={(model) =>
                    void validateProviderModel(provider, model)
                  }
                  onRemoveModel={(modelId) =>
                    removeProviderModel(provider.id, modelId)
                  }
                />
              );
            })()
          : null}
      </section>

      {!selectedProviderId ? (
        <LlmModelPoolSection
          orderedModelReferences={orderedModelReferences}
          saving={saving}
          loadError={loadError}
          message={message}
          onMove={moveCandidate}
          onSave={() => void saveRouting()}
        />
      ) : null}
    </div>
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

  const defaultModelRow = isRecord(rawRecord.defaultModel)
    ? rawRecord.defaultModel
    : isRecord(rawRecord.default_model)
      ? rawRecord.default_model
      : null;
  const providerId =
    defaultModelRow?.providerId ?? defaultModelRow?.provider_id;
  const modelId = defaultModelRow?.modelId ?? defaultModelRow?.model_id;
  const defaultModel =
    typeof providerId === "string" && typeof modelId === "string"
      ? { providerId, modelId: normalizePersistedModelId(modelId) }
      : null;
  const rawCandidateOrder = Array.isArray(rawRecord.candidateOrder)
    ? rawRecord.candidateOrder
    : Array.isArray(rawRecord.candidate_order)
      ? rawRecord.candidate_order
      : [];
  const candidateOrder = normalizeCandidateOrder(
    providers,
    rawCandidateOrder.flatMap((entry) => {
      if (!isRecord(entry)) return [];
      const providerId = entry.providerId ?? entry.provider_id;
      const modelId = entry.modelId ?? entry.model_id;
      return typeof providerId === "string" && typeof modelId === "string"
        ? [{ providerId, modelId: normalizePersistedModelId(modelId) }]
        : [];
    }),
  );

  return {
    version: typeof rawRecord.version === "number" ? rawRecord.version : 1,
    schemaVersion:
      typeof rawRecord.schemaVersion === "number" ? rawRecord.schemaVersion : 6,
    providers,
    candidateOrder:
      candidateOrder.length > 0 || !defaultModel
        ? candidateOrder
        : normalizeCandidateOrder(providers, [defaultModel]),
    defaultModel,
  };
}

function normalizePersistedModelId(model: string): string {
  return model === "mimo-vl-7b-experimental" ? "MiMo-V2.5-Pro" : model;
}
