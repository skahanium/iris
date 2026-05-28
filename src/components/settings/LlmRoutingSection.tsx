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

import {
  invokeErrorMessage,
  llmCredentialService,
} from "@/lib/credentials";
import {
  credentialDelete,
  credentialHas,
  credentialSet,
  llmConfigApplyDeepseekDefaults,
  llmConfigGet,
  llmConfigSet,
  llmConfigTest,
} from "@/lib/ipc";
import { notifyLlmConfigChanged } from "@/lib/llm-ipc";
import { SCENE_META } from "@/lib/ai/scene-types";
import type { AiScene } from "@/types/ai";
import {
  AI_SCENES,
  DEFAULT_LLM_ROUTING,
  isCustomProviderId,
  type ContextStrategy,
  type LlmConfigGetResponse,
  type LlmRoutingConfig,
  type ProviderOverride,
  type SceneRoute,
} from "@/types/llm";

const FALLBACK_PROVIDERS: LlmConfigGetResponse["providers"] = [
  { id: "deepseek", name: "DeepSeek", default_model: "deepseek-v4-flash" },
];

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
  const [keyInputs, setKeyInputs] = useState<Record<string, string>>({});
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
  /** 忽略过期的凭据批量检查，避免覆盖刚保存的状态 */
  const keyStatusEpochRef = useRef(0);

  const refreshKeyStatus = useCallback(async (providerIds: string[]) => {
    const epoch = ++keyStatusEpochRef.current;
    setKeysLoading(true);
    try {
      const configured: Record<string, boolean> = {};
      await Promise.all(
        providerIds.map(async (id) => {
          try {
            configured[id] = await credentialHas(llmCredentialService(id));
          } catch {
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
        "当前在浏览器中打开，无法调用 Tauri 后端。请关闭此标签页，使用 npx tauri dev 启动的 Iris 桌面窗口。",
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
      const routing = normalizeRouting(res.routing);
      setData({ ...res, routing });
      setRouting(routing);
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

  const saveKey = async (providerId: string) => {
    const value = keyInputs[providerId]?.trim();
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
        setMessage(
          `${label} Key 写入后校验失败，请重试；输入内容已保留。`,
        );
        return;
      }
      setKeyInputs((prev) => ({ ...prev, [providerId]: "" }));
      setKeyConfigured((prev) => ({ ...prev, [providerId]: true }));
      setMessage(
        `${label} Key 已保存到系统凭据管理器（输入框已清空以保护隐私）。`,
      );
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

  const updateScene = (
    scene: AiScene,
    patch: Partial<{ providerId: string; model: string; thinking: boolean }>,
  ) => {
    if (!routing) return;
    const current = routing.scenes[scene] ?? {
      providerId: "deepseek",
      model: "deepseek-v4-flash",
      thinking: false,
    };
    setRouting({
      ...routing,
      scenes: {
        ...routing.scenes,
        [scene]: { ...current, ...patch },
      },
    });
  };

  const updateStrategy = (scene: AiScene, strategy: ContextStrategy) => {
    if (!routing) return;
    setRouting({
      ...routing,
      contextStrategy: { ...routing.contextStrategy, [scene]: strategy },
    });
  };

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
              default_model:
                next.defaultModel?.trim() || p.default_model,
            }
          : p,
      ),
    });
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
    const used = AI_SCENES.some(
      (scene) => routing.scenes[scene]?.providerId === providerId,
    );
    if (used) {
      setMessage(`无法删除：仍有场景使用 ${providerId}，请先改场景厂商。`);
      return;
    }
    const { [providerId]: _removed, ...rest } = routing.providers;
    setRouting({ ...routing, providers: rest });
    setData({
      ...data,
      providers: data.providers.filter((p) => p.id !== providerId),
    });
    setMessage(null);
  };

  const modelsFor = (providerId: string) =>
    data?.catalog.filter((m) => m.providerId === providerId) ?? [];

  const applyDeepseek = async () => {
    const defaults = await llmConfigApplyDeepseekDefaults();
    setRouting(defaults);
    setMessage("已应用 DeepSeek 推荐路由");
    notifyLlmConfigChanged();
  };

  const saveRouting = async () => {
    if (!routing) return;
    setSaving(true);
    setMessage(null);
    try {
      await llmConfigSet(routing);
      setMessage("路由已保存");
      notifyLlmConfigChanged();
    } finally {
      setSaving(false);
    }
  };

  const testProvider = async (providerId: string) => {
    setTesting(providerId);
    setTestResults((prev) => {
      const next = { ...prev };
      delete next[providerId];
      return next;
    });
    try {
      const result = await llmConfigTest(providerId);
      setTestResults((prev) => ({
        ...prev,
        [providerId]: { ok: result.ok, message: result.message },
      }));
    } catch (err) {
      setTestResults((prev) => ({
        ...prev,
        [providerId]: {
          ok: false,
          message: invokeErrorMessage(err),
        },
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
          支持 DeepSeek 与多个 OpenAI 兼容自定义端点；API Key 仅存系统凭据管理器。
        </p>
        {loadError ? (
          <p className="mt-2 text-xs text-amber-600">
            未能从后端读取配置：{loadError}。已显示默认项；请确认使用{" "}
            <strong>npx tauri dev</strong> 启动的 Iris 窗口（不是单独打开
            http://127.0.0.1:1420），然后点「重试」。
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="ml-1 h-auto px-1 text-xs text-amber-700"
              onClick={() => void load()}
            >
              重试
            </Button>
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
            厂商与凭据
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

        {data.providers
          .filter((p) => p.id === "deepseek")
          .map((p) => (
            <ProviderCredentialCard
              key={p.id}
              provider={p}
              routing={routing}
              keyConfigured={keyConfigured}
              keyInputs={keyInputs}
              keySaving={keySaving}
              testing={testing}
              testResult={testResults[p.id]}
              onKeyInput={(id, value) =>
                setKeyInputs((prev) => ({ ...prev, [id]: value }))
              }
              onSaveKey={(id) => void saveKey(id)}
              onClearKey={(id) => void clearKey(id)}
              onTest={(id) => void testProvider(id)}
              onBaseUrl={(id, url) =>
                updateProviderOverride(id, { baseUrl: url.trim() || null })
              }
            />
          ))}

        {data.providers.filter((p) => isCustomProviderId(p.id)).length > 0 ? (
          <p className="text-[10px] text-muted-foreground">
            自定义 OpenAI 兼容（/v1/chat/completions）
          </p>
        ) : (
          <p className="text-[10px] text-muted-foreground">
            暂无自定义端点；可添加本地代理、Groq 等兼容服务。
          </p>
        )}

        {data.providers
          .filter((p) => isCustomProviderId(p.id))
          .map((p) => (
            <div key={p.id} className="space-y-2">
              <ProviderCredentialCard
                provider={p}
                routing={routing}
                keyConfigured={keyConfigured}
                keyInputs={keyInputs}
                keySaving={keySaving}
                testing={testing}
                testResult={testResults[p.id]}
                custom
                onKeyInput={(id, value) =>
                  setKeyInputs((prev) => ({ ...prev, [id]: value }))
                }
                onSaveKey={(id) => void saveKey(id)}
                onClearKey={(id) => void clearKey(id)}
                onTest={(id) => void testProvider(id)}
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
              />
              <Button
                type="button"
                size="sm"
                variant="ghost"
                className="h-7 text-xs text-destructive"
                onClick={() => removeCustomProvider(p.id)}
              >
                移除此端点
              </Button>
            </div>
          ))}
      </div>

      <div className="space-y-2">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="text-xs font-medium text-muted-foreground">
            场景模型路由
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
        {AI_SCENES.map((scene) => {
          const meta = SCENE_META[scene];
          const route = routing.scenes[scene];
          const providerId = route?.providerId ?? "deepseek";
          const models = modelsFor(providerId);
          return (
            <div
              key={scene}
              className="grid gap-2 rounded-lg border border-border/50 p-2 sm:grid-cols-[minmax(5rem,1fr)_1fr_1fr_1fr]"
            >
              <span className="self-center text-xs font-medium">
                {meta.label}
              </span>
              <Select
                value={providerId}
                onValueChange={(v) =>
                  updateScene(scene, {
                    providerId: v,
                    model:
                      modelsFor(v)[0]?.id ?? "deepseek-v4-flash",
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
              {isCustomProviderId(providerId) ? (
                <Input
                  className="h-8 text-xs"
                  placeholder={
                    routing.providers[providerId]?.defaultModel ??
                    "模型 ID，如 gpt-4o-mini"
                  }
                  value={route?.model ?? ""}
                  onChange={(e) =>
                    updateScene(scene, { model: e.target.value })
                  }
                />
              ) : (
                <Select
                  value={route?.model ?? ""}
                  onValueChange={(v) => updateScene(scene, { model: v })}
                >
                  <SelectTrigger className="h-8 text-xs">
                    <SelectValue placeholder="模型" />
                  </SelectTrigger>
                  <SelectContent>
                    {models.map((m) => (
                      <SelectItem key={m.id} value={m.id}>
                        {m.displayName}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              )}
              <Select
                value={routing.contextStrategy[scene] ?? "hybrid"}
                onValueChange={(v) =>
                  updateStrategy(scene, v as ContextStrategy)
                }
              >
                <SelectTrigger className="h-8 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="hybrid">混合检索</SelectItem>
                  <SelectItem value="long_context">长上下文</SelectItem>
                </SelectContent>
              </Select>
            </div>
          );
        })}
        <p className="text-[10px] text-muted-foreground">
          deepseek-reasoner 与 Flash 非思考模式勿在同一会话混用，以免影响前缀缓存。
        </p>
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
  keyInputs: Record<string, string>;
  keySaving: string | null;
  testing: string | null;
  testResult?: { ok: boolean; message: string };
  custom?: boolean;
  onKeyInput: (id: string, value: string) => void;
  onSaveKey: (id: string) => void;
  onClearKey: (id: string) => void;
  onTest: (id: string) => void;
  onBaseUrl: (id: string, url: string) => void;
  onLabel?: (id: string, label: string) => void;
  onDefaultModel?: (id: string, model: string) => void;
}

function ProviderCredentialCard({
  provider: p,
  routing,
  keyConfigured,
  keyInputs,
  keySaving,
  testing,
  testResult,
  custom = false,
  onKeyInput,
  onSaveKey,
  onClearKey,
  onTest,
  onBaseUrl,
  onLabel,
  onDefaultModel,
}: ProviderCredentialCardProps) {
  const override = routing.providers[p.id];
  return (
    <div className="rounded-lg border border-border/60 bg-surface-inset/30 p-3">
      <div className="mb-2 flex items-center justify-between gap-2">
        <span className="text-xs font-medium">
          {custom ? override?.label?.trim() || p.name : p.name}
        </span>
        {keyConfigured[p.id] ? (
          <span className="text-[10px] text-emerald-600">Key 已配置</span>
        ) : (
          <span className="text-[10px] text-amber-600">未配置 Key</span>
        )}
      </div>
      {custom && onLabel ? (
        <Input
          className="mb-2 h-8 text-xs"
          placeholder="显示名称"
          value={override?.label ?? ""}
          onChange={(e) => onLabel(p.id, e.target.value)}
        />
      ) : null}
      <p className="mb-1.5 text-[10px] text-muted-foreground">
        保存后输入框会清空，右侧应显示「Key 已配置」。
      </p>
      <div className="flex gap-2">
        <Input
          type="password"
          className="h-8 text-xs"
          placeholder="API Key…"
          value={keyInputs[p.id] ?? ""}
          onChange={(e) => onKeyInput(p.id, e.target.value)}
        />
        <Button
          type="button"
          size="sm"
          className="h-8 shrink-0"
          disabled={keySaving === p.id}
          onClick={() => onSaveKey(p.id)}
        >
          {keySaving === p.id ? "保存中…" : "保存"}
        </Button>
        {keyConfigured[p.id] ? (
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="h-8 shrink-0"
            onClick={() => onClearKey(p.id)}
          >
            清除
          </Button>
        ) : null}
      </div>
      <Input
        className="mt-2 h-8 text-xs"
        placeholder={
          p.id === "deepseek"
            ? "https://api.deepseek.com（留空用官方默认）"
            : "Base URL，如 https://api.openai.com/v1"
        }
        value={override?.baseUrl ?? ""}
        onChange={(e) => onBaseUrl(p.id, e.target.value)}
      />
      {custom && onDefaultModel ? (
        <Input
          className="mt-2 h-8 text-xs"
          placeholder="默认模型 ID（测试连接与场景未填时使用）"
          value={override?.defaultModel ?? ""}
          onChange={(e) => onDefaultModel(p.id, e.target.value)}
        />
      ) : null}
      <div className="mt-2 flex flex-wrap items-center gap-2">
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="h-7 text-xs"
          disabled={testing === p.id}
          onClick={() => onTest(p.id)}
        >
          {testing === p.id ? "测试中…" : "测试连接"}
        </Button>
        {testResult ? (
          <span
            className={
              testResult.ok
                ? "text-xs text-emerald-600"
                : "text-xs text-destructive"
            }
          >
            {testResult.message}
          </span>
        ) : null}
      </div>
    </div>
  );
}

/** 兼容 IPC 返回 camelCase / snake_case 混用 */
function normalizeRouting(raw: LlmRoutingConfig | undefined): LlmRoutingConfig {
  if (!raw) return DEFAULT_LLM_ROUTING;
  const scenes: Record<string, SceneRoute> = {};
  for (const scene of AI_SCENES) {
    const r = raw.scenes[scene] as SceneRoute & {
      provider_id?: string;
    };
    if (!r) continue;
    scenes[scene] = {
      providerId: r.providerId ?? r.provider_id ?? "deepseek",
      model: r.model ?? "deepseek-v4-flash",
      thinking: r.thinking ?? false,
    };
  }
  const providers: LlmRoutingConfig["providers"] = {};
  for (const [id, p] of Object.entries(raw.providers ?? {})) {
    const row = p as ProviderOverride & {
      base_url?: string | null;
      default_model?: string | null;
    };
    providers[id] = {
      baseUrl: row.baseUrl ?? row.base_url ?? null,
      label: row.label ?? null,
      defaultModel: row.defaultModel ?? row.default_model ?? null,
    };
  }
  return {
    version: raw.version ?? 1,
    providers,
    scenes: { ...DEFAULT_LLM_ROUTING.scenes, ...scenes },
    contextStrategy: {
      ...DEFAULT_LLM_ROUTING.contextStrategy,
      ...raw.contextStrategy,
    },
  };
}

