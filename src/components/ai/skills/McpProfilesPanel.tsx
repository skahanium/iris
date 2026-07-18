import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";

import { Button } from "@/components/ui/button";
import { invokeErrorMessage } from "@/lib/credentials";
import {
  credentialDelete,
  credentialSet,
  credentialStatus,
  webEvidenceProviderDelete,
  webEvidenceProviderDiagnostics,
  webEvidenceProvidersList,
  webEvidenceProviderToggle,
  webEvidenceProviderUpsert,
  type WebEvidenceProviderDiagnostics,
  type WebEvidenceProviderInput,
  type WebEvidenceProviderSummary,
} from "@/lib/ipc";

import { McpProfileCard, type McpCredentialSave } from "./McpProfileCard";
import type { McpProviderPreset } from "./mcpProviderPresets";

interface McpProfilesPanelProps {
  open: boolean;
  onProvidersChanged?: () => void;
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
  onProvidersChanged,
}: McpProfilesPanelProps) {
  const [providers, setProviders] = useState<WebEvidenceProviderSummary[]>([]);
  const [diagnostics, setDiagnostics] = useState<DiagnosticsByProvider>({});
  const [credentialConfiguredByService, setCredentialConfiguredByService] =
    useState<Record<string, boolean>>({});
  const [draft, setDraft] = useState<WebEvidenceProviderSummary | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const diagnosticsEpochRef = useRef(0);

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
      const nextProviders = await webEvidenceProvidersList();
      setProviders(nextProviders);
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
      setDraft(null);
      await load();
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

  if (!isTauri()) {
    return <></>;
  }

  return (
    <section
      data-testid="mcp-provider-panel"
      className="space-y-3 border-t border-border/60 pt-4"
    >
      <header className="space-y-3">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h3 className="text-sm font-medium">MCP 联网证据提供方</h3>
            <p className="mt-1 text-xs text-muted-foreground">
              将 MCP 显式接入 web.search / web.fetch；联网搜索只使用当前选择的
              MCP 提供方。
            </p>
          </div>
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={loading || saving}
            onClick={() => {
              invalidateDiagnostics();
              setDraft(createDraftSummary());
            }}
          >
            添加 MCP 提供方
          </Button>
        </div>
      </header>

      {draft ? (
        <McpProfileCard
          provider={draft}
          diagnostics={diagnostics[draft.id]}
          credentialConfiguredByService={credentialConfiguredByService}
          saving={saving}
          persisted={false}
          onSave={saveProvider}
          onToggle={(enabled) => {
            invalidateDiagnostics();
            setDraft((current) =>
              current ? { ...current, enabled } : current,
            );
          }}
          onDelete={() => {
            invalidateDiagnostics();
            setDraft(null);
          }}
          onClearCredential={() => {
            setMessage("草稿尚未保存，没有可清除的 API Key。");
          }}
          onDiagnostics={() => {
            setMessage("请先保存 MCP 提供方，再执行实时诊断。");
          }}
          onConfigurationChanged={invalidateDiagnostics}
        />
      ) : null}

      {mcpProviders.length > 0 ? (
        <div className="space-y-3">
          {mcpProviders.map((provider) => (
            <McpProfileCard
              key={provider.id}
              provider={provider}
              diagnostics={diagnostics[provider.id]}
              credentialConfiguredByService={credentialConfiguredByService}
              saving={saving}
              onSave={saveProvider}
              onToggle={(enabled) => toggleProvider(provider.id, enabled)}
              onDelete={() => deleteProvider(provider.id)}
              onClearCredential={clearCredential}
              onDiagnostics={() => void runDiagnostics(provider.id)}
              onConfigurationChanged={invalidateDiagnostics}
            />
          ))}
        </div>
      ) : !draft ? (
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
