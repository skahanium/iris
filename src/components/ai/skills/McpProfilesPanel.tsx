import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";

import { Button } from "@/components/ui/button";
import { invokeErrorMessage } from "@/lib/credentials";
import {
  credentialDelete,
  credentialSet,
  credentialStatus,
  mcpCapabilityBindingDelete,
  mcpCapabilityBindingsList,
  mcpCapabilityBindingUpsert,
  mcpReadOnlyToolsDiscover,
  webEvidenceProviderDelete,
  webEvidenceProviderDiagnostics,
  webEvidenceProvidersList,
  webEvidenceProviderToggle,
  webEvidenceProviderUpsert,
  webSearchRouteGet,
  webSearchRouteSet,
  type WebEvidenceProviderDiagnostics,
  type WebEvidenceProviderInput,
  type WebEvidenceProviderSummary,
  type McpCapabilityBindingSummary,
  type McpReadOnlyToolCandidate,
} from "@/lib/ipc";

import { McpProfileCard, type McpCredentialSave } from "./McpProfileCard";
import { McpProviderDetail } from "./McpProviderDetail";
import {
  mcpListMappingShortLabel,
  mcpListTransportShortLabel,
  orderMcpProvidersForSearchRoute,
} from "./mcpProviderListUi";
import type { McpProviderPreset } from "./mcpProviderPresets";
import type { ManagementProviderChrome } from "@/components/settings/managementProviderChrome";

interface McpProfilesPanelProps {
  open: boolean;
  selectedProviderId?: string | null;
  onSelectedProviderIdChange?: (providerId: string | null) => void;
  onProvidersChanged?: () => void;
  onProviderChromeChange?: (chrome: ManagementProviderChrome | null) => void;
}

type DiagnosticsByProvider = Record<string, WebEvidenceProviderDiagnostics>;

function credentialServicesFromRefsJson(raw: string): string[] {
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return [];
    }
    const record = parsed as Record<string, unknown>;
    const services: string[] = [];
    for (const section of ["headers", "env"] as const) {
      const bindings = record[section];
      if (
        !bindings ||
        typeof bindings !== "object" ||
        Array.isArray(bindings)
      ) {
        continue;
      }
      for (const value of Object.values(bindings as Record<string, unknown>)) {
        if (typeof value === "string") {
          const service = value.replace(/^credential:\/\//, "").trim();
          if (service) services.push(service);
          continue;
        }
        if (!value || typeof value !== "object" || Array.isArray(value))
          continue;
        const binding = value as Record<string, unknown>;
        const ref = binding.credential ?? binding.service ?? binding.ref;
        if (typeof ref === "string") {
          const service = ref.replace(/^credential:\/\//, "").trim();
          if (service) services.push(service);
        }
      }
    }
    return [...new Set(services)];
  } catch {
    return [];
  }
}

function mappingStatus(
  searchMapping?: string | null,
  fetchMapping?: string | null,
): string {
  if (searchMapping && fetchMapping) return "complete";
  if (searchMapping || fetchMapping) return "partial";
  return "missing";
}

function createDraftSummary(
  preset?: McpProviderPreset,
): WebEvidenceProviderSummary {
  const id = `mcp-${preset?.id ?? "custom"}-${Date.now()}`;
  const transportKind = preset?.transportKind ?? "https";
  const env = Object.fromEntries(
    (preset?.plainEnv ?? [])
      .map((row) => [row.name, row.value] as const)
      .filter(([, value]) => value.trim().length > 0),
  );
  const transportConfigJson =
    transportKind === "stdio"
      ? JSON.stringify(
          {
            preset_id: preset?.id,
            command: preset?.command ?? "",
            args: preset?.args ?? [],
            ...(Object.keys(env).length > 0 ? { env } : {}),
          },
          null,
          2,
        )
      : JSON.stringify(
          {
            preset_id: preset?.id,
            url: preset?.url ?? "",
            allow_localhost_dev: preset?.allowLocalhostDev === true,
          },
          null,
          2,
        );
  const headers = Object.fromEntries(
    (preset?.credentials ?? [])
      .filter((item) => item.target === "header")
      .map((item) => [
        item.name,
        {
          credential: `credential://${item.service}`,
          ...(item.scheme ? { scheme: item.scheme } : {}),
          ...(item.optional === true
            ? { optional: item.optional === true }
            : {}),
        },
      ]),
  );
  const credentialEnv = Object.fromEntries(
    (preset?.credentials ?? [])
      .filter((item) => item.target === "env")
      .map((item) => [
        item.name,
        item.optional === true
          ? {
              credential: `credential://${item.service}`,
              optional: item.optional === true,
            }
          : `credential://${item.service}`,
      ]),
  );
  const credentialRefsJson = JSON.stringify(
    {
      ...(Object.keys(headers).length > 0 ? { headers } : {}),
      ...(Object.keys(credentialEnv).length > 0 ? { env: credentialEnv } : {}),
    },
    null,
    2,
  );
  const nextMappingStatus = mappingStatus(
    preset?.searchMapping,
    preset?.fetchMapping,
  );
  return {
    id,
    name: preset?.providerName ?? "MCP 联网证据提供方",
    providerKind: "mcp",
    enabled: false,
    transportKind,
    transportConfigJson,
    credentialRefsJson,
    searchMapping: preset?.searchMapping ?? null,
    fetchMapping: preset?.fetchMapping ?? null,
    mappingStatus: nextMappingStatus,
    diagnosticStatus: "disabled",
    isNative: false,
    editable: true,
    hasSearchMapping: Boolean(preset?.searchMapping),
    hasFetchMapping: Boolean(preset?.fetchMapping),
  };
}

export function McpProfilesPanel({
  open,
  selectedProviderId = null,
  onSelectedProviderIdChange,
  onProvidersChanged,
  onProviderChromeChange,
}: McpProfilesPanelProps) {
  const [providers, setProviders] = useState<WebEvidenceProviderSummary[]>([]);
  const [searchRouteIds, setSearchRouteIds] = useState<string[]>([]);
  const [diagnostics, setDiagnostics] = useState<DiagnosticsByProvider>({});
  const [credentialConfiguredByService, setCredentialConfiguredByService] =
    useState<Record<string, boolean>>({});
  const [draft, setDraft] = useState<WebEvidenceProviderSummary | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [externalBindings, setExternalBindings] = useState<
    McpCapabilityBindingSummary[]
  >([]);
  const [discoveredReadTools, setDiscoveredReadTools] = useState<
    McpReadOnlyToolCandidate[]
  >([]);
  const [externalToolsBusy, setExternalToolsBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [localSelectedProviderId, setLocalSelectedProviderId] = useState<
    string | null
  >(selectedProviderId ?? null);
  const diagnosticsEpochRef = useRef(0);

  useEffect(() => {
    setLocalSelectedProviderId(selectedProviderId ?? null);
  }, [selectedProviderId]);

  const setSelectedProvider = (providerId: string | null) => {
    setLocalSelectedProviderId(providerId);
    onSelectedProviderIdChange?.(providerId);
  };

  const invalidateDiagnostics = useCallback(() => {
    diagnosticsEpochRef.current += 1;
    setDiagnostics({});
  }, []);

  const refreshCredentialStatuses = useCallback(
    async (items: WebEvidenceProviderSummary[]) => {
      const services = [
        ...new Set(
          items.flatMap((item) =>
            credentialServicesFromRefsJson(item.credentialRefsJson),
          ),
        ),
      ];
      if (services.length === 0) {
        setCredentialConfiguredByService({});
        return;
      }
      const entries = await Promise.all(
        services.map(async (service) => {
          try {
            const status = await credentialStatus(service);
            return [service, status.configured] as const;
          } catch {
            return [service, false] as const;
          }
        }),
      );
      setCredentialConfiguredByService(Object.fromEntries(entries));
    },
    [],
  );

  const load = useCallback(async () => {
    if (!isTauri()) return;
    setLoading(true);
    setMessage(null);
    invalidateDiagnostics();
    try {
      const [nextProviders, route] = await Promise.all([
        webEvidenceProvidersList(),
        Promise.resolve()
          .then(() => webSearchRouteGet())
          .catch(() => ({ candidateProviderIds: [] })),
      ]);
      setProviders(nextProviders);
      setSearchRouteIds(route?.candidateProviderIds ?? []);
      await refreshCredentialStatuses(nextProviders);
    } catch (error) {
      setMessage(invokeErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }, [invalidateDiagnostics, refreshCredentialStatuses]);

  useEffect(() => {
    invalidateDiagnostics();
    if (open) void load();
  }, [invalidateDiagnostics, load, open]);

  const mcpProviders = useMemo(
    () => providers.filter((provider) => provider.providerKind === "mcp"),
    [providers],
  );

  const listedMcpProviders = useMemo(
    () => orderMcpProvidersForSearchRoute(mcpProviders, searchRouteIds),
    [mcpProviders, searchRouteIds],
  );

  const orderedSearchProviders = useMemo(
    () =>
      listedMcpProviders
        .filter((item) => item.searchRouteRole !== undefined)
        .map((item) => item.provider),
    [listedMcpProviders],
  );

  const moveSearchRouteProvider = async (
    providerId: string,
    direction: -1 | 1,
  ) => {
    const from = orderedSearchProviders.findIndex(
      (provider) => provider.id === providerId,
    );
    const to = from + direction;
    if (from < 0 || to < 0 || to >= orderedSearchProviders.length) return;
    const next = [...orderedSearchProviders];
    [next[from], next[to]] = [next[to]!, next[from]!];
    setSaving(true);
    setMessage(null);
    try {
      const route = await webSearchRouteSet({
        candidateProviderIds: next.map((provider) => provider.id),
      });
      setSearchRouteIds(route.candidateProviderIds);
      onProvidersChanged?.();
    } catch (error) {
      setMessage(invokeErrorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const saveProvider = async (
    input: WebEvidenceProviderInput,
    credentialSaves: McpCredentialSave[],
  ) => {
    setSaving(true);
    setMessage(null);
    invalidateDiagnostics();
    try {
      for (const credential of credentialSaves) {
        await credentialSet(credential.service, credential.value);
      }
      await webEvidenceProviderUpsert(input);
      const savedId = input.id;
      setDraft(null);
      await load();
      setSelectedProvider(savedId);
      onProvidersChanged?.();
      setMessage(
        credentialSaves.length > 0
          ? "MCP 提供方已保存，API Key 已写入系统凭据。"
          : "MCP 提供方已保存。",
      );
    } catch (error) {
      setMessage(invokeErrorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const toggleProvider = async (providerId: string, enabled: boolean) => {
    setSaving(true);
    setMessage(null);
    invalidateDiagnostics();
    try {
      await webEvidenceProviderToggle(providerId, enabled);
      await load();
      onProvidersChanged?.();
    } catch (error) {
      setMessage(invokeErrorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const deleteProvider = async (providerId: string) => {
    setSaving(true);
    setMessage(null);
    invalidateDiagnostics();
    try {
      await webEvidenceProviderDelete(providerId);
      await load();
      onProvidersChanged?.();
    } catch (error) {
      setMessage(invokeErrorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const clearCredential = async (service: string) => {
    setSaving(true);
    setMessage(null);
    invalidateDiagnostics();
    try {
      await credentialDelete(service);
      await load();
      onProvidersChanged?.();
      setMessage(
        "已清除保存的 API Key；可保持为空并主动使用匿名额度，或重新输入原始 Key。",
      );
    } catch (error) {
      setMessage(invokeErrorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const runDiagnostics = async (providerId: string) => {
    setMessage(null);
    invalidateDiagnostics();
    const epoch = diagnosticsEpochRef.current;
    try {
      const result = await webEvidenceProviderDiagnostics(providerId);
      if (open && diagnosticsEpochRef.current === epoch) {
        setDiagnostics({ [providerId]: result });
      }
    } catch (error) {
      if (open && diagnosticsEpochRef.current === epoch) {
        setMessage(invokeErrorMessage(error));
      }
    }
  };

  const activeDetailId = localSelectedProviderId ?? (draft ? draft.id : null);
  const detailProvider =
    draft && draft.id === activeDetailId
      ? draft
      : mcpProviders.find((item) => item.id === activeDetailId);

  const refreshExternalBindings = useCallback(async (providerId: string) => {
    if (typeof mcpCapabilityBindingsList !== "function") return;
    const bindings = await mcpCapabilityBindingsList(providerId);
    setExternalBindings(bindings);
  }, []);

  useEffect(() => {
    setDiscoveredReadTools([]);
    if (!open || !detailProvider || (draft && draft.id === detailProvider.id)) {
      setExternalBindings([]);
      return;
    }
    void refreshExternalBindings(detailProvider.id).catch(() =>
      setExternalBindings([]),
    );
  }, [detailProvider, draft, open, refreshExternalBindings]);

  const discoverReadOnlyTools = async (providerId: string) => {
    if (typeof mcpReadOnlyToolsDiscover !== "function") return;
    setExternalToolsBusy(true);
    setMessage(null);
    try {
      const result = await mcpReadOnlyToolsDiscover(providerId);
      setDiscoveredReadTools(result.tools);
      setMessage(
        result.rejectedCount > 0
          ? `已发现 ${result.tools.length} 个可审查只读工具；另有 ${result.rejectedCount} 个副作用或不受支持工具已排除。`
          : `已发现 ${result.tools.length} 个可审查只读工具。`,
      );
    } catch (error) {
      setMessage(invokeErrorMessage(error));
    } finally {
      setExternalToolsBusy(false);
    }
  };

  const bindReadOnlyTool = async (
    providerId: string,
    tool: McpReadOnlyToolCandidate,
  ) => {
    if (typeof mcpCapabilityBindingUpsert !== "function") return;
    const sanitizedSchema = JSON.stringify(tool.inputSchema, null, 2);
    if (
      !window.confirm(
        [
          "请审核并确认此外部只读工具：",
          `提供方：${tool.providerDisplayName}`,
          `真实工具稳定 ID：${tool.name}`,
          `提供方配置哈希：${tool.providerConfigHash}`,
          `绑定配置哈希：${tool.bindingConfigHash}`,
          "清洗后的输入 Schema：",
          sanitizedSchema,
          "",
          "仅当你信任此 MCP 提供方，并已独立确认该工具不会写入、发送、删除、修改日历、启动进程或读取秘密时继续。服务端的只读标记只是声明，Iris 无法验证其实现。确认将它加入外部只读白名单？",
        ].join("\n"),
      )
    ) {
      return;
    }
    setExternalToolsBusy(true);
    setMessage(null);
    try {
      await mcpCapabilityBindingUpsert({
        providerId,
        mcpToolName: tool.name,
        inputSchema: tool.inputSchema,
        argumentMapping: {},
        riskClass: tool.riskClass,
        readOnly: tool.readOnly,
        userTrusted: true,
        attestedBindingConfigHash: tool.bindingConfigHash,
      });
      await refreshExternalBindings(providerId);
      setMessage("只读工具绑定已保存；仍需在 Composer 为每个 Run 单独授权。");
    } catch (error) {
      setMessage(invokeErrorMessage(error));
    } finally {
      setExternalToolsBusy(false);
    }
  };

  const deleteReadOnlyBinding = async (
    providerId: string,
    bindingId: string,
  ) => {
    if (typeof mcpCapabilityBindingDelete !== "function") return;
    setExternalToolsBusy(true);
    setMessage(null);
    try {
      await mcpCapabilityBindingDelete(bindingId);
      await refreshExternalBindings(providerId);
      setMessage("只读工具绑定已删除。");
    } catch (error) {
      setMessage(invokeErrorMessage(error));
    } finally {
      setExternalToolsBusy(false);
    }
  };

  const providerChromePayload = useMemo((): ManagementProviderChrome | null => {
    if (!detailProvider) return null;
    return {
      label: detailProvider.name || "MCP 联网证据提供方",
      detail: `${mcpListTransportShortLabel(detailProvider.transportKind)} · ${mcpListMappingShortLabel(detailProvider.mappingStatus)}`,
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- keyed by provider fields, not object identity
  }, [
    detailProvider?.id,
    detailProvider?.mappingStatus,
    detailProvider?.name,
    detailProvider?.transportKind,
  ]);

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

  if (!isTauri()) {
    return <></>;
  }

  return (
    <section
      data-testid="mcp-provider-panel"
      className="space-y-3 border-t border-border/60 pt-4"
    >
      {!detailProvider ? (
        <div className="flex flex-wrap items-start justify-between gap-3">
          <p className="max-w-xl text-xs text-muted-foreground">
            将 MCP 显式接入 web.search / web.fetch；联网搜索按下列主备顺序切换。
          </p>
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={loading || saving}
            onClick={() => {
              invalidateDiagnostics();
              const nextDraft = createDraftSummary();
              setDraft(nextDraft);
              setSelectedProvider(nextDraft.id);
            }}
          >
            添加 MCP 提供方
          </Button>
        </div>
      ) : null}

      {detailProvider ? (
        <McpProviderDetail
          provider={detailProvider}
          diagnostics={diagnostics[detailProvider.id]}
          credentialConfiguredByService={credentialConfiguredByService}
          saving={saving}
          persisted={!(draft && draft.id === detailProvider.id)}
          onSave={saveProvider}
          onToggle={(enabled) => {
            if (draft && draft.id === detailProvider.id) {
              invalidateDiagnostics();
              setDraft((current) =>
                current ? { ...current, enabled } : current,
              );
              return;
            }
            void toggleProvider(detailProvider.id, enabled);
          }}
          onDelete={() => {
            if (draft && draft.id === detailProvider.id) {
              invalidateDiagnostics();
              setDraft(null);
              setSelectedProvider(null);
              return;
            }
            void deleteProvider(detailProvider.id).then(() =>
              setSelectedProvider(null),
            );
          }}
          onClearCredential={clearCredential}
          onDiagnostics={() => void runDiagnostics(detailProvider.id)}
          onConfigurationChanged={invalidateDiagnostics}
        >
          {!(draft && draft.id === detailProvider.id) ? (
            <div
              className="space-y-3 rounded-md border border-border/60 p-3"
              data-testid="mcp-read-only-tool-bindings"
            >
              <div className="flex items-start justify-between gap-3">
                <div>
                  <p className="text-xs font-medium">逐 Run 外部只读工具</p>
                  <p className="mt-1 text-xs text-muted-foreground">
                    这里只建立由你审核并信任的 external.read
                    白名单；服务端只读标记不是安全证明。启用提供方不会自动授权，Composer
                    会在发送后清除本次选择。
                  </p>
                </div>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={
                    externalToolsBusy || !detailProvider.enabled || saving
                  }
                  onClick={() => void discoverReadOnlyTools(detailProvider.id)}
                >
                  发现只读工具
                </Button>
              </div>
              {externalBindings.map((binding) => (
                <div
                  key={binding.id}
                  className="flex items-center justify-between gap-3 rounded-md border border-border-subtle px-2 py-1.5 text-xs"
                >
                  <div>
                    <p className="font-medium">{binding.mcpToolName}</p>
                    <p className="text-muted-foreground">
                      {binding.domainOperation || binding.outputMapping
                        ? "已退役的旧领域映射 · 仅兼容读取，不可用于新 Run"
                        : "external.read · 参数同名映射"}{" "}
                      ·{" "}
                      {binding.providerEnabled && binding.configMatches
                        ? "诊断通过"
                        : "配置已漂移或提供方停用"}
                    </p>
                  </div>
                  <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    disabled={externalToolsBusy}
                    onClick={() =>
                      void deleteReadOnlyBinding(detailProvider.id, binding.id)
                    }
                  >
                    删除绑定
                  </Button>
                </div>
              ))}
              {discoveredReadTools.map((tool) => {
                const bound = externalBindings.some(
                  (binding) => binding.mcpToolName === tool.name,
                );
                return (
                  <div
                    key={tool.name}
                    data-testid={`mcp-external-read-tool-${tool.name}`}
                    className="space-y-3 rounded-md border border-border-subtle bg-surface-inset/20 p-3 text-xs"
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div>
                        <p className="font-medium">{tool.name}</p>
                        <p className="text-muted-foreground">
                          {bound
                            ? "已有 binding；若为旧领域映射，请先删除后重新审核为 external.read"
                            : "可审核为逐 Run external.read；不会创建领域路由或自动授权"}
                        </p>
                      </div>
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        data-testid={`mcp-external-bind-${tool.name}`}
                        disabled={externalToolsBusy || bound}
                        onClick={() =>
                          void bindReadOnlyTool(detailProvider.id, tool)
                        }
                      >
                        {bound ? "已绑定" : "审核并信任为只读"}
                      </Button>
                    </div>
                    <details className="space-y-2 border-t border-border-subtle pt-3">
                      <summary
                        data-testid={`mcp-external-advanced-${tool.name}`}
                        className="cursor-pointer text-xs text-muted-foreground"
                      >
                        高级：只读 schema 与哈希
                      </summary>
                      <div className="space-y-2 pt-2">
                        <pre
                          data-testid={`mcp-external-schema-${tool.name}`}
                          className="max-h-48 overflow-auto rounded-md bg-surface-inset/40 p-2 text-[11px] text-muted-foreground"
                        >
                          {JSON.stringify(tool.inputSchema, null, 2)}
                        </pre>
                        <p className="text-muted-foreground">
                          Provider config hash：{tool.providerConfigHash}
                        </p>
                        <p className="text-muted-foreground">
                          Binding config hash：{tool.bindingConfigHash}
                        </p>
                      </div>
                    </details>
                  </div>
                );
              })}
            </div>
          ) : null}
        </McpProviderDetail>
      ) : activeDetailId ? (
        <div className="space-y-2 rounded-md border border-border/55 bg-background/60 p-3 text-xs text-muted-foreground">
          <p>找不到该 MCP 提供方，可能已被删除。</p>
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="h-7"
            onClick={() => {
              setDraft(null);
              setSelectedProvider(null);
            }}
          >
            返回列表
          </Button>
        </div>
      ) : null}

      {!activeDetailId && listedMcpProviders.length > 0 ? (
        <div className="space-y-3">
          {listedMcpProviders.map(({ provider, searchRouteRole }) => {
            const routeIndex = orderedSearchProviders.findIndex(
              (candidate) => candidate.id === provider.id,
            );
            return (
              <McpProfileCard
                key={provider.id}
                provider={provider}
                surface="list"
                onSelect={() => setSelectedProvider(provider.id)}
                searchRouteRole={searchRouteRole}
                canMoveSearchRouteUp={routeIndex > 0}
                canMoveSearchRouteDown={
                  routeIndex >= 0 &&
                  routeIndex < orderedSearchProviders.length - 1
                }
                onMoveSearchRoute={
                  searchRouteRole
                    ? (direction) =>
                        void moveSearchRouteProvider(provider.id, direction)
                    : undefined
                }
                credentialConfiguredByService={credentialConfiguredByService}
                saving={saving}
                onSave={saveProvider}
                onToggle={(enabled) => toggleProvider(provider.id, enabled)}
                onDelete={() => deleteProvider(provider.id)}
                onClearCredential={clearCredential}
                onDiagnostics={() => void runDiagnostics(provider.id)}
                onConfigurationChanged={invalidateDiagnostics}
              />
            );
          })}
        </div>
      ) : !activeDetailId && !draft ? (
        <p className="rounded-md border border-dashed border-border/70 px-3 py-3 text-xs text-muted-foreground">
          还没有配置 MCP 提供方。点击添加 MCP 提供方后，可选择预设或自定义服务。
        </p>
      ) : null}

      {message ? (
        <p className="text-xs text-muted-foreground">{message}</p>
      ) : null}
    </section>
  );
}
