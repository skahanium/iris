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
  llmConfigApplyDeepseekDefaults,
  llmConfigGet,
  llmConfigSet,
  llmConfigTest,
} from "@/lib/ipc";
import { notifyLlmConfigChanged } from "@/lib/llm-events";
import type { CapabilitySlot } from "@/types/ai";
import {
  CAPABILITY_SLOTS,
  DEFAULT_LLM_ROUTING,
  isCustomProviderId,
  type LlmConfigGetResponse,
  type LlmRoutingConfig,
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
  { id: "ollama", name: "Ollama", default_model: "llama3.2" },
];

const SLOT_META: Record<CapabilitySlot, { label: string; detail: string }> = {
  fast: { label: "Fast", detail: "短问答、轻量检索、默认对话" },
  writer: { label: "Writer", detail: "改写、续写、章节与文档写作" },
  reasoner: { label: "Reasoner", detail: "研究、引用核查、复杂论证" },
  long_context: { label: "Long context", detail: "长文档与大上下文分析" },
  vision: { label: "Vision", detail: "图片输入与视觉问答" },
  agent_tools: { label: "Agent tools", detail: "工具循环、Skills 管理与检索" },
  embedding: { label: "Embedding", detail: "本地向量与索引能力预留" },
  reranker: { label: "Reranker", detail: "检索重排能力预留" },
  local_private: { label: "Local private", detail: "本地私有模型优先" },
};

type ProbeKind = "connection" | "vision" | "tools";

function nextCustomProviderId(existing: Iterable<string>): string {
  const set = new Set(existing);
  if (!set.has("custom")) return "custom";
  let n = 2;
  while (set.has(`custom_${n}`)) n += 1;
  return `custom_${n}`;
}

interface LlmRoutingSectionProps {
  open: boolean;
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
  const [loadError, setLoadError] = useState<string | null>(null);
  const [keysLoading, setKeysLoading] = useState(false);
  const [keySaving, setKeySaving] = useState<string | null>(null);
  const keyStatusEpochRef = useRef(0);

  const refreshKeyStatus = useCallback(async (providerIds: string[]) => {
    const epoch = ++keyStatusEpochRef.current;
    setKeysLoading(true);
    try {
      const configured: Record<string, boolean> = {};
      await Promise.all(
        providerIds.map(async (id) => {
          try {
            configured[id] =
              id === "ollama" ||
              (await credentialHas(llmCredentialService(id)));
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

  const load = useCallback(async () => {
    setLoadError(null);
    if (!isTauri()) {
      setLoadError(
        "当前浏览器标签页无法调用 Tauri 后端，请在 Iris 桌面窗口中配置。",
      );
      setRouting(DEFAULT_LLM_ROUTING);
      setData({
        routing: DEFAULT_LLM_ROUTING,
        providers: FALLBACK_PROVIDERS,
        catalog: [],
      });
      return;
    }
    try {
      const res = await llmConfigGet();
      const normalized = normalizeRouting(res.routing);
      setData({ ...res, routing: normalized });
      setRouting(normalized);
      void refreshKeyStatus(res.providers.map((p) => p.id));
    } catch (err) {
      setLoadError(invokeErrorMessage(err));
      setRouting(DEFAULT_LLM_ROUTING);
      setData({
        routing: DEFAULT_LLM_ROUTING,
        providers: FALLBACK_PROVIDERS,
        catalog: [],
      });
    }
  }, [refreshKeyStatus]);

  useEffect(() => {
    if (open) void load();
  }, [open, load]);

  const updateProviderOverride = (
    providerId: string,
    patch: Partial<ProviderOverride>,
  ) => {
    if (!routing || !data) return;
    const prev = routing.providers[providerId] ?? {
      baseUrl: null,
      label: null,
      defaultModel: null,
    };
    const next: ProviderOverride = { ...prev, ...patch };
    setRouting({
      ...routing,
      providers: { ...routing.providers, [providerId]: next },
    });
    setData({
      ...data,
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

  const saveKey = async (providerId: string) => {
    const value = keyInputsRef.current[providerId]?.trim();
    if (!value) return;
    const service = llmCredentialService(providerId);
    const label =
      data?.providers.find((p) => p.id === providerId)?.name ?? providerId;

    keyStatusEpochRef.current += 1;
    setKeySaving(providerId);
    setMessage(null);
    try {
      await credentialSet(service, value);
      const verified = await credentialHas(service);
      if (!verified) {
        setMessage(`${label} Key 写入后校验失败，请重试。`);
        return;
      }
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
    const label =
      data?.providers.find((p) => p.id === providerId)?.name ?? providerId;
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

  const addCustomProvider = () => {
    if (!routing || !data) return;
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
      defaultModel: "default",
    };
    setRouting({
      ...routing,
      providers: { ...routing.providers, [id]: entry },
    });
    setData({
      ...data,
      providers: [
        ...data.providers,
        { id, name: label, default_model: "default" },
      ],
    });
    void refreshKeyStatus([id]);
  };

  const removeCustomProvider = (providerId: string) => {
    if (!routing || !data || !isCustomProviderId(providerId)) return;
    const used = CAPABILITY_SLOTS.some(
      (slot) => routing.slots[slot]?.providerId === providerId,
    );
    if (used) {
      setMessage(`无法删除：仍有能力槽使用 ${providerId}，请先改厂商。`);
      return;
    }
    const { [providerId]: _removed, ...rest } = routing.providers;
    void _removed;
    setRouting({ ...routing, providers: rest });
    setData({
      ...data,
      providers: data.providers.filter((p) => p.id !== providerId),
    });
    setMessage(null);
  };

  const updateSlot = (
    slot: CapabilitySlot,
    patch: Partial<{ providerId: string; model: string; thinking: boolean }>,
  ) => {
    if (!routing) return;
    const current = routing.slots[slot] ?? DEFAULT_LLM_ROUTING.slots[slot];
    setRouting({
      ...routing,
      slots: {
        ...routing.slots,
        [slot]: { ...current, ...patch },
      },
    });
  };

  const modelsFor = (providerId: string) =>
    data?.catalog.filter((m) => m.providerId === providerId) ?? [];

  const modelById = (modelId: string): ModelCatalogEntry | undefined =>
    data?.catalog.find((m) => m.id === modelId);

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

  const applyDeepseek = async () => {
    const defaults = await llmConfigApplyDeepseekDefaults();
    setRouting(normalizeRouting(defaults));
    setMessage("已应用 DeepSeek 推荐能力槽路由");
    notifyLlmConfigChanged();
  };

  const testProvider = async (providerId: string, probe: ProbeKind) => {
    const key = `${providerId}:${probe}`;
    setTesting(key);
    setTestResults((prev) => {
      const next = { ...prev };
      delete next[key];
      return next;
    });
    try {
      const result = await llmConfigTest(providerId);
      setTestResults((prev) => ({
        ...prev,
        [key]: {
          ok: result.ok,
          message:
            probe === "connection"
              ? result.message
              : `${probe} probe: ${result.message}`,
        },
      }));
    } catch (err) {
      setTestResults((prev) => ({
        ...prev,
        [key]: { ok: false, message: invokeErrorMessage(err) },
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
        <h3 className="text-sm font-medium">AI 连接</h3>
        <p className="mt-0.5 text-xs text-muted-foreground">
          静态模型目录 + 手动模型 ID；API Key 仅存系统凭据管理器。
        </p>
        {loadError ? (
          <p className="mt-2 text-xs text-amber-600">
            未能从后端读取配置：{loadError}
          </p>
        ) : null}
        {keysLoading ? (
          <p className="mt-1 text-[10px] text-muted-foreground">
            正在检查各厂商凭据…
          </p>
        ) : null}
      </div>

      <div className="space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="text-xs font-medium text-muted-foreground">
            厂商、凭据与探测
          </p>
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="h-7 text-xs"
            onClick={addCustomProvider}
          >
            添加自定义端点
          </Button>
        </div>

        <div className="grid gap-3 lg:grid-cols-2">
          {data.providers.map((provider) => (
            <ProviderCredentialCard
              key={provider.id}
              provider={provider}
              routing={routing}
              keyConfigured={keyConfigured}
              keyInputsRef={keyInputsRef}
              keySaving={keySaving}
              testing={testing}
              testResults={testResults}
              custom={isCustomProviderId(provider.id)}
              onKeyInput={(id, value) => {
                keyInputsRef.current[id] = value;
                setKeyInputTouch((n) => n + 1);
              }}
              onSaveKey={(id) => void saveKey(id)}
              onClearKey={(id) => void clearKey(id)}
              onTest={(id, probe) => void testProvider(id, probe)}
              onBaseUrl={(id, url) =>
                updateProviderOverride(id, { baseUrl: url.trim() || null })
              }
              onLabel={(id, label) =>
                updateProviderOverride(id, { label: label.trim() || null })
              }
              onDefaultModel={(id, model) =>
                updateProviderOverride(id, {
                  defaultModel: model.trim() || null,
                })
              }
              onRemove={(id) => removeCustomProvider(id)}
            />
          ))}
        </div>
      </div>

      <div className="space-y-2">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="text-xs font-medium text-muted-foreground">
            能力槽模型路由
          </p>
          <Button
            type="button"
            size="sm"
            variant="secondary"
            className="h-7 text-xs"
            onClick={() => void applyDeepseek()}
          >
            DeepSeek 推荐
          </Button>
        </div>

        <div className="space-y-2">
          {CAPABILITY_SLOTS.map((slot) => {
            const route =
              routing.slots[slot] ?? DEFAULT_LLM_ROUTING.slots[slot];
            const providerId = route.providerId;
            const models = modelsFor(providerId);
            const catalogModel = modelById(route.model);
            return (
              <div
                key={slot}
                className="grid gap-2 rounded-md border border-border/50 bg-background/60 p-2 xl:grid-cols-[minmax(8rem,0.9fr)_1fr_1.2fr_1.5fr]"
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
                      model:
                        modelsFor(value)[0]?.id ??
                        routing.providers[value]?.defaultModel ??
                        "default",
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
                {isCustomProviderId(providerId) || models.length === 0 ? (
                  <Input
                    className="h-8 text-xs"
                    placeholder="模型 ID，如 gpt-4o-mini"
                    value={route.model}
                    onChange={(event) =>
                      updateSlot(slot, { model: event.target.value })
                    }
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
                          {model.displayName}
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
      </div>

      <div className="flex items-center gap-2">
        <Button
          type="button"
          size="sm"
          disabled={saving}
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

interface ProviderCredentialCardProps {
  provider: { id: string; name: string };
  routing: LlmRoutingConfig;
  keyConfigured: Record<string, boolean>;
  keyInputsRef: React.RefObject<Record<string, string>>;
  keySaving: string | null;
  testing: string | null;
  testResults: Record<string, { ok: boolean; message: string }>;
  custom?: boolean;
  onKeyInput: (id: string, value: string) => void;
  onSaveKey: (id: string) => void;
  onClearKey: (id: string) => void;
  onTest: (id: string, probe: ProbeKind) => void;
  onBaseUrl: (id: string, url: string) => void;
  onLabel: (id: string, label: string) => void;
  onDefaultModel: (id: string, model: string) => void;
  onRemove: (id: string) => void;
}

function ProviderCredentialCard({
  provider,
  routing,
  keyConfigured,
  keyInputsRef,
  keySaving,
  testing,
  testResults,
  custom = false,
  onKeyInput,
  onSaveKey,
  onClearKey,
  onTest,
  onBaseUrl,
  onLabel,
  onDefaultModel,
  onRemove,
}: ProviderCredentialCardProps) {
  const override = routing.providers[provider.id];
  const displayName = custom
    ? override?.label?.trim() || provider.name
    : provider.name;
  const keyless = provider.id === "ollama";

  return (
    <div className="rounded-md border border-border/60 bg-surface-inset/25 p-3">
      <div className="mb-2 flex items-center justify-between gap-2">
        <span className="text-xs font-medium">{displayName}</span>
        {keyConfigured[provider.id] || keyless ? (
          <span className="text-[10px] text-emerald-600">
            {keyless ? "本地端点" : "Key 已配置"}
          </span>
        ) : (
          <span className="text-[10px] text-amber-600">未配置 Key</span>
        )}
      </div>
      {custom ? (
        <Input
          className="mb-2 h-8 text-xs"
          placeholder="显示名称"
          value={override?.label ?? ""}
          onChange={(event) => onLabel(provider.id, event.target.value)}
        />
      ) : null}
      {!keyless ? (
        <div className="flex gap-2">
          <Input
            type="password"
            className="h-8 text-xs"
            placeholder="API Key…"
            value={keyInputsRef.current?.[provider.id] ?? ""}
            onChange={(event) => onKeyInput(provider.id, event.target.value)}
          />
          <Button
            type="button"
            size="sm"
            className="h-8 shrink-0"
            disabled={keySaving === provider.id}
            onClick={() => onSaveKey(provider.id)}
          >
            {keySaving === provider.id ? "保存中…" : "保存"}
          </Button>
          {keyConfigured[provider.id] ? (
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="h-8 shrink-0"
              onClick={() => onClearKey(provider.id)}
            >
              清除
            </Button>
          ) : null}
        </div>
      ) : null}
      <Input
        className="mt-2 h-8 text-xs"
        placeholder={
          provider.id === "ollama"
            ? "http://127.0.0.1:11434"
            : "Base URL（留空使用官方默认）"
        }
        value={override?.baseUrl ?? ""}
        onChange={(event) => onBaseUrl(provider.id, event.target.value)}
      />
      {custom ? (
        <Input
          className="mt-2 h-8 text-xs"
          placeholder="默认模型 ID"
          value={override?.defaultModel ?? ""}
          onChange={(event) => onDefaultModel(provider.id, event.target.value)}
        />
      ) : null}
      <div className="mt-2 flex flex-wrap items-center gap-2">
        {(["connection", "vision", "tools"] as ProbeKind[]).map((probe) => {
          const key = `${provider.id}:${probe}`;
          const result = testResults[key];
          return (
            <Button
              key={probe}
              type="button"
              size="sm"
              variant="outline"
              className="h-7 text-xs"
              disabled={testing === key}
              onClick={() => onTest(provider.id, probe)}
            >
              {testing === key ? "测试中…" : probe}
              {result ? (
                <span
                  className={
                    result.ok
                      ? "ml-1 text-emerald-600"
                      : "ml-1 text-destructive"
                  }
                >
                  {result.ok ? "ok" : "fail"}
                </span>
              ) : null}
            </Button>
          );
        })}
        {custom ? (
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="h-7 text-xs text-destructive"
            onClick={() => onRemove(provider.id)}
          >
            移除
          </Button>
        ) : null}
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
    };
    providers[id] = {
      baseUrl: row.baseUrl ?? row.base_url ?? null,
      label: row.label ?? null,
      defaultModel: row.defaultModel ?? row.default_model ?? null,
    };
  }

  const slots: LlmRoutingConfig["slots"] = { ...DEFAULT_LLM_ROUTING.slots };
  for (const slot of CAPABILITY_SLOTS) {
    const rawSlots = raw.slots as Partial<Record<CapabilitySlot, SlotRoute>>;
    const route = rawSlots?.[slot];
    if (!route) continue;
    const row = route as SlotRoute & { provider_id?: string };
    slots[slot] = {
      providerId: row.providerId ?? row.provider_id ?? "deepseek",
      model: row.model ?? DEFAULT_LLM_ROUTING.slots[slot].model,
      thinking: row.thinking ?? false,
    };
  }

  return {
    version: raw.version ?? 1,
    schemaVersion: raw.schemaVersion ?? 2,
    providers,
    slots,
    scenes: raw.scenes ?? DEFAULT_LLM_ROUTING.scenes,
    contextStrategy: {
      ...DEFAULT_LLM_ROUTING.contextStrategy,
      ...raw.contextStrategy,
    },
  };
}
