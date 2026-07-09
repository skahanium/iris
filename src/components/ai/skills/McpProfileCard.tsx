import { useEffect, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import type {
  WebEvidenceProviderDiagnostics,
  WebEvidenceProviderInput,
  WebEvidenceProviderSummary,
} from "@/lib/ipc";
import { cn } from "@/lib/utils";

import {
  findMcpProviderPreset,
  MCP_PROVIDER_PRESETS,
  type McpProviderPreset,
  type McpTransportKind,
} from "./mcpProviderPresets";

export interface McpCredentialSave {
  service: string;
  value: string;
}

interface McpProfileCardProps {
  provider: WebEvidenceProviderSummary;
  diagnostics?: WebEvidenceProviderDiagnostics | null;
  saving?: boolean;
  persisted?: boolean;
  onSave: (
    input: WebEvidenceProviderInput,
    credentialSaves: McpCredentialSave[],
  ) => void | Promise<void>;
  onToggle: (enabled: boolean) => void | Promise<void>;
  onDelete: () => void | Promise<void>;
  onDiagnostics: (liveCheck?: boolean) => void | Promise<void>;
}

interface HttpsConfigState {
  url: string;
  allowLocalhostDev: boolean;
}

interface StdioConfigState {
  command: string;
  argsText: string;
  envRows: PlainEnvRow[];
}

interface PlainEnvRow {
  id: string;
  name: string;
  value: string;
  label?: string;
  placeholder?: string;
}

interface CredentialRefRow {
  id: string;
  target: "header" | "env";
  name: string;
  ref: string;
  label?: string;
  scheme?: string;
  placeholder?: string;
  optional?: boolean;
  secretValue: string;
}

function editableTransportKind(
  value: string | null | undefined,
): McpTransportKind {
  return value === "stdio" ? "stdio" : "https";
}

function parseJsonRecord(
  raw: string | null | undefined,
): Record<string, unknown> {
  if (!raw?.trim()) return {};
  try {
    const parsed: unknown = JSON.parse(raw);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : {};
  } catch {
    return {};
  }
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value.map((item) => (typeof item === "string" ? item : String(item)))
    : [];
}

function parseHttpsConfig(raw: string | null | undefined): HttpsConfigState {
  const parsed = parseJsonRecord(raw);
  return {
    url: typeof parsed.url === "string" ? parsed.url : "",
    allowLocalhostDev: parsed.allow_localhost_dev === true,
  };
}

function parsePlainEnvRows(raw: string | null | undefined): PlainEnvRow[] {
  const parsed = parseJsonRecord(raw);
  const env = parsed.env;
  if (!env || typeof env !== "object" || Array.isArray(env)) return [];
  return Object.entries(env as Record<string, unknown>)
    .filter(([, value]) => typeof value === "string")
    .map(([name, value], index) => ({
      id: `plain-env-${index}-${name}`,
      name,
      value: value as string,
    }));
}

function parseStdioConfig(raw: string | null | undefined): StdioConfigState {
  const parsed = parseJsonRecord(raw);
  return {
    command: typeof parsed.command === "string" ? parsed.command : "",
    argsText: stringArray(parsed.args).join("\n"),
    envRows: parsePlainEnvRows(raw),
  };
}

function credentialService(raw: unknown): string {
  if (typeof raw === "string") return raw.replace(/^credential:\/\//, "");
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return "";
  const record = raw as Record<string, unknown>;
  const service = record.credential ?? record.service ?? record.ref;
  return typeof service === "string"
    ? service.replace(/^credential:\/\//, "")
    : "";
}

function credentialScheme(raw: unknown): string | undefined {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return undefined;
  const scheme = (raw as Record<string, unknown>).scheme;
  return typeof scheme === "string" && scheme.trim()
    ? scheme.trim()
    : undefined;
}

function credentialOptional(raw: unknown): boolean {
  return (
    raw != null &&
    typeof raw === "object" &&
    !Array.isArray(raw) &&
    (raw as Record<string, unknown>).optional === true
  );
}

function parseCredentialRows(
  raw: string | null | undefined,
): CredentialRefRow[] {
  const parsed = parseJsonRecord(raw);
  const rows: CredentialRefRow[] = [];
  const headers = parsed.headers;
  if (headers && typeof headers === "object" && !Array.isArray(headers)) {
    Object.entries(headers as Record<string, unknown>).forEach(
      ([name, value], index) => {
        rows.push({
          id: `credential-header-${index}-${name}`,
          target: "header",
          name,
          ref: credentialService(value),
          scheme: credentialScheme(value),
          optional: credentialOptional(value),
          secretValue: "",
        });
      },
    );
  }
  const env = parsed.env;
  if (env && typeof env === "object" && !Array.isArray(env)) {
    Object.entries(env as Record<string, unknown>).forEach(
      ([name, value], index) => {
        rows.push({
          id: `credential-env-${index}-${name}`,
          target: "env",
          name,
          ref: credentialService(value),
          optional: credentialOptional(value),
          secretValue: "",
        });
      },
    );
  }
  if (rows.length === 0) {
    Object.entries(parsed)
      .filter(([, value]) => typeof value === "string")
      .forEach(([name, value], index) => {
        rows.push({
          id: `credential-legacy-${index}-${name}`,
          target: "env",
          name,
          ref: credentialService(value),
          secretValue: "",
        });
      });
  }
  return rows;
}

function rowsFromPreset(preset: McpProviderPreset): CredentialRefRow[] {
  return preset.credentials.map((item, index) => ({
    id: `credential-${preset.id}-${index}-${item.name}`,
    target: item.target,
    name: item.name,
    ref: item.service,
    label: item.label,
    scheme: item.scheme,
    placeholder: item.placeholder,
    optional: item.optional,
    secretValue: "",
  }));
}

function plainEnvRowsFromPreset(preset: McpProviderPreset): PlainEnvRow[] {
  return preset.plainEnv.map((item, index) => ({
    id: `plain-env-${preset.id}-${index}-${item.name}`,
    name: item.name,
    value: item.value,
    label: item.label,
    placeholder: item.placeholder,
  }));
}

function argsTextToArray(value: string): string[] {
  return value
    .split(/\r?\n/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function mappingToolName(
  raw: string | null | undefined,
  fallback = "",
): string {
  const value = raw?.trim();
  if (!value) return fallback;
  try {
    const parsed = JSON.parse(value) as { tool?: unknown; tool_name?: unknown };
    const tool =
      typeof parsed.tool === "string" ? parsed.tool : parsed.tool_name;
    return typeof tool === "string" ? tool : value;
  } catch {
    return value;
  }
}

function mappingForSave(raw: string, toolName: string): string | null {
  const tool = toolName.trim();
  if (!tool) {
    const noMapping: null = null;
    return noMapping;
  }
  try {
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      return JSON.stringify({ ...parsed, tool });
    }
  } catch {
    // fall through to simple mapping
  }
  return JSON.stringify({ tool });
}

function credentialRowsToJson(rows: CredentialRefRow[]): string {
  const headers: Record<string, unknown> = {};
  const env: Record<string, unknown> = {};
  for (const row of rows) {
    const name = row.name.trim();
    const ref = row.ref.trim().replace(/^credential:\/\//, "");
    if (!name || !ref) continue;
    if (row.target === "header") {
      headers[name] = {
        credential: `credential://${ref}`,
        ...(row.scheme ? { scheme: row.scheme } : {}),
        ...(row.optional === true ? { optional: row.optional === true } : {}),
      };
    } else {
      env[name] = row.optional
        ? {
            credential: `credential://${ref}`,
            optional: row.optional === true,
          }
        : `credential://${ref}`;
    }
  }
  return JSON.stringify(
    {
      ...(Object.keys(headers).length > 0 ? { headers } : {}),
      ...(Object.keys(env).length > 0 ? { env } : {}),
    },
    null,
    2,
  );
}

function plainEnvRowsToRecord(
  rows: PlainEnvRow[],
): Record<string, string> | undefined {
  const env = Object.fromEntries(
    rows
      .map((row) => [row.name.trim(), row.value.trim()] as const)
      .filter(([name, value]) => name.length > 0 && value.length > 0),
  );
  return Object.keys(env).length > 0 ? env : undefined;
}

function statusText(enabled: boolean): string {
  return enabled ? "已启用" : "已停用";
}

function transportLabel(kind: McpTransportKind): string {
  return kind === "stdio" ? "本地命令 (stdio)" : "HTTPS 服务";
}

function mappingStatusText(status: string): string {
  switch (status) {
    case "complete":
      return "搜索和读取均已映射";
    case "partial":
      return "部分映射";
    case "missing":
      return "未配置映射";
    default:
      return status;
  }
}

function diagnosticStatusText(status: string): string {
  switch (status) {
    case "ready":
      return "可参与调度";
    case "needs_mapping":
      return "需要补全映射";
    case "disabled":
      return "已停用";
    default:
      return status;
  }
}

function checkStatusText(status: string): string {
  switch (status) {
    case "pass":
    case "ok":
      return "正常";
    case "warn":
    case "warning":
      return "需确认";
    case "fail":
    case "error":
      return "异常";
    case "missing":
      return "缺失";
    default:
      return status;
  }
}

function checkLabelText(label: string): string {
  switch (label) {
    case "configured":
    case "provider":
      return "提供方记录";
    case "enabled":
      return "启用状态";
    case "transport":
      return "连接配置";
    case "credential":
      return "凭据状态";
    case "searchMapping":
    case "search_mapping":
      return "搜索映射";
    case "fetchMapping":
    case "fetch_mapping":
      return "网页读取映射";
    case "providerKind":
      return "提供方类型";
    case "registry":
      return "提供方注册表";
    case "liveConnection":
      return "实时连接";
    case "searchToolLive":
      return "搜索工具";
    case "searchSmokeLive":
      return "搜索调用";
    case "searchResultParseLive":
      return "结果解析";
    case "fetchToolLive":
      return "网页读取工具";
    default:
      return label;
  }
}

function credentialStateText(rows: CredentialRefRow[]): string {
  if (rows.length === 0) return "不需要凭据";
  const hasPendingKey = rows.some((row) => row.secretValue.trim().length > 0);
  if (hasPendingKey) return "本次保存会更新 Key";
  if (rows.every((row) => row.optional)) return "匿名模式";
  return "必填凭据缺失或待填写";
}

function presetIdFromProvider(provider: WebEvidenceProviderSummary): string {
  const parsed = parseJsonRecord(provider.transportConfigJson);
  return typeof parsed.preset_id === "string" ? parsed.preset_id : "custom";
}

export function McpProfileCard({
  provider,
  diagnostics,
  saving = false,
  persisted = true,
  onSave,
  onToggle,
  onDelete,
  onDiagnostics,
}: McpProfileCardProps) {
  const [name, setName] = useState(provider.name || "MCP 联网证据提供方");
  const [enabled, setEnabled] = useState(provider.enabled);
  const [presetId, setPresetId] = useState(presetIdFromProvider(provider));
  const [transportKind, setTransportKind] = useState<McpTransportKind>(
    editableTransportKind(provider.transportKind),
  );
  const [httpsConfig, setHttpsConfig] = useState(() =>
    parseHttpsConfig(provider.transportConfigJson),
  );
  const [stdioConfig, setStdioConfig] = useState(() =>
    parseStdioConfig(provider.transportConfigJson),
  );
  const [credentialRows, setCredentialRows] = useState(() =>
    parseCredentialRows(provider.credentialRefsJson),
  );
  const [searchMappingRaw, setSearchMappingRaw] = useState(
    provider.searchMapping ?? "",
  );
  const [fetchMappingRaw, setFetchMappingRaw] = useState(
    provider.fetchMapping ?? "",
  );
  const [searchTool, setSearchTool] = useState(
    mappingToolName(provider.searchMapping),
  );
  const [fetchTool, setFetchTool] = useState(
    mappingToolName(provider.fetchMapping),
  );

  useEffect(() => {
    setName(provider.name || "MCP 联网证据提供方");
    setEnabled(provider.enabled);
    setPresetId(presetIdFromProvider(provider));
    setTransportKind(editableTransportKind(provider.transportKind));
    setHttpsConfig(parseHttpsConfig(provider.transportConfigJson));
    setStdioConfig(parseStdioConfig(provider.transportConfigJson));
    setCredentialRows(parseCredentialRows(provider.credentialRefsJson));
    setSearchMappingRaw(provider.searchMapping ?? "");
    setFetchMappingRaw(provider.fetchMapping ?? "");
    setSearchTool(mappingToolName(provider.searchMapping));
    setFetchTool(mappingToolName(provider.fetchMapping));
  }, [provider]);

  const selectedPreset = useMemo(
    () => findMcpProviderPreset(presetId === "custom" ? undefined : presetId),
    [presetId],
  );
  const diagnosticLines = diagnostics?.checks ?? [];
  const hasLiveDiagnostics = diagnosticLines.some((check) =>
    [
      "liveConnection",
      "searchToolLive",
      "searchSmokeLive",
      "searchResultParseLive",
      "fetchToolLive",
    ].includes(check.label),
  );
  const credentialState = credentialStateText(credentialRows);

  const applyPreset = (preset: McpProviderPreset) => {
    setPresetId(preset.id);
    setName(preset.providerName);
    setTransportKind(preset.transportKind);
    setHttpsConfig({
      url: preset.url ?? "",
      allowLocalhostDev: preset.allowLocalhostDev === true,
    });
    setStdioConfig({
      command: preset.command ?? "",
      argsText: (preset.args ?? []).join("\n"),
      envRows: plainEnvRowsFromPreset(preset),
    });
    setCredentialRows(rowsFromPreset(preset));
    setSearchMappingRaw(preset.searchMapping ?? "");
    setFetchMappingRaw(preset.fetchMapping ?? "");
    setSearchTool(mappingToolName(preset.searchMapping));
    setFetchTool(mappingToolName(preset.fetchMapping));
  };

  const addCredentialRow = () => {
    setCredentialRows((rows) => [
      ...rows,
      {
        id: `credential-${Date.now()}`,
        target: transportKind === "https" ? "header" : "env",
        name: transportKind === "https" ? "Authorization" : "API_KEY",
        ref: "iris.mcp.custom",
        scheme: transportKind === "https" ? "bearer" : undefined,
        secretValue: "",
      },
    ]);
  };

  const addPlainEnvRow = () => {
    setStdioConfig((current) => ({
      ...current,
      envRows: [
        ...current.envRows,
        { id: `plain-env-${Date.now()}`, name: "", value: "" },
      ],
    }));
  };

  const updateCredentialRow = (
    rowId: string,
    patch: Partial<CredentialRefRow>,
  ) => {
    setCredentialRows((rows) =>
      rows.map((row) => (row.id === rowId ? { ...row, ...patch } : row)),
    );
  };

  const updatePlainEnvRow = (rowId: string, patch: Partial<PlainEnvRow>) => {
    setStdioConfig((current) => ({
      ...current,
      envRows: current.envRows.map((row) =>
        row.id === rowId ? { ...row, ...patch } : row,
      ),
    }));
  };

  const removeCredentialRow = (rowId: string) => {
    setCredentialRows((rows) => rows.filter((row) => row.id !== rowId));
  };

  const removePlainEnvRow = (rowId: string) => {
    setStdioConfig((current) => ({
      ...current,
      envRows: current.envRows.filter((row) => row.id !== rowId),
    }));
  };

  const handleSave = () => {
    const plainEnv = plainEnvRowsToRecord(stdioConfig.envRows);
    const transportConfigJson =
      transportKind === "stdio"
        ? JSON.stringify(
            {
              preset_id: presetId === "custom" ? undefined : presetId,
              command: stdioConfig.command.trim(),
              args: argsTextToArray(stdioConfig.argsText),
              ...(plainEnv ? { env: plainEnv } : {}),
            },
            null,
            2,
          )
        : JSON.stringify(
            {
              preset_id: presetId === "custom" ? undefined : presetId,
              url: httpsConfig.url.trim(),
              allow_localhost_dev: httpsConfig.allowLocalhostDev,
            },
            null,
            2,
          );
    const credentialSaves = credentialRows
      .map((row) => ({
        service: row.ref.trim(),
        value: row.secretValue.trim(),
      }))
      .filter(
        (row): row is McpCredentialSave =>
          row.service.length > 0 && row.value.length > 0,
      );
    void onSave(
      {
        id: provider.id,
        name: name.trim() || "MCP 联网证据提供方",
        providerKind: "mcp",
        enabled,
        transportKind,
        transportConfigJson,
        credentialRefsJson: credentialRowsToJson(credentialRows),
        searchMapping: mappingForSave(searchMappingRaw, searchTool),
        fetchMapping: mappingForSave(fetchMappingRaw, fetchTool),
      },
      credentialSaves,
    );
  };

  const statusClassName = enabled
    ? "border-emerald-500/25 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
    : "border-border bg-muted/40 text-muted-foreground";

  return (
    <article className="space-y-4 rounded-lg border border-border/65 bg-background/55 p-4">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 space-y-1">
          <div className="flex flex-wrap items-center gap-2">
            <p className="text-sm font-semibold text-foreground">
              {name.trim() || "MCP 联网证据提供方"}
            </p>
            <span
              className={cn(
                "rounded-full border px-2 py-0.5 text-[11px] font-medium",
                statusClassName,
              )}
            >
              {statusText(enabled)}
            </span>
          </div>
          <p className="text-xs text-muted-foreground">
            连接：{transportLabel(transportKind)} ·{" "}
            {mappingStatusText(provider.mappingStatus)} ·{" "}
            {diagnosticStatusText(provider.diagnosticStatus)}
          </p>
          {selectedPreset ? (
            <p className="max-w-3xl text-xs text-muted-foreground">
              {selectedPreset.description}
            </p>
          ) : null}
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            size="sm"
            variant={enabled ? "secondary" : "outline"}
            disabled={saving}
            onClick={() => {
              const next = !enabled;
              setEnabled(next);
              void onToggle(next);
            }}
          >
            {enabled ? "停用" : "启用"}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={saving || !persisted}
            title={!persisted ? "请先保存提供方，再测试连接" : undefined}
            onClick={() => void onDiagnostics(true)}
          >
            测试连接
          </Button>
        </div>
      </header>

      <section className="max-w-xl">
        <label className="space-y-1 text-xs font-medium text-foreground">
          快速预设
          <Select
            value={presetId}
            disabled={saving}
            onValueChange={(value) => {
              const preset = findMcpProviderPreset(value);
              if (preset) applyPreset(preset);
              else setPresetId("custom");
            }}
          >
            <SelectTrigger className="h-9 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="custom">自定义 MCP 服务</SelectItem>
              {MCP_PROVIDER_PRESETS.map((preset) => (
                <SelectItem key={preset.id} value={preset.id}>
                  {preset.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </label>
      </section>

      <div className="grid gap-3 md:grid-cols-2">
        <label className="space-y-1 text-xs font-medium text-foreground">
          提供方名称
          <Input
            value={name}
            disabled={saving}
            placeholder="MCP 联网证据提供方"
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        <label className="space-y-1 text-xs font-medium text-foreground">
          连接方式
          <Select
            value={transportKind}
            disabled={saving}
            onValueChange={(value) =>
              setTransportKind(editableTransportKind(value))
            }
          >
            <SelectTrigger className="h-9 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="https">HTTPS 服务</SelectItem>
              <SelectItem value="stdio">本地命令 (stdio)</SelectItem>
            </SelectContent>
          </Select>
        </label>
      </div>

      {transportKind === "stdio" ? (
        <section className="space-y-3">
          <div className="grid gap-3 md:grid-cols-[minmax(0,0.8fr)_minmax(0,1.2fr)]">
            <label className="space-y-1 text-xs font-medium text-foreground">
              stdio 启动命令
              <Input
                value={stdioConfig.command}
                disabled={saving}
                spellCheck={false}
                placeholder="例如：npx"
                onChange={(event) =>
                  setStdioConfig((current) => ({
                    ...current,
                    command: event.target.value,
                  }))
                }
              />
            </label>
            <label className="space-y-1 text-xs font-medium text-foreground">
              启动参数
              <Textarea
                value={stdioConfig.argsText}
                disabled={saving}
                rows={3}
                spellCheck={false}
                placeholder={"每行一个参数，例如：\n-y\nmcp-searxng"}
                onChange={(event) =>
                  setStdioConfig((current) => ({
                    ...current,
                    argsText: event.target.value,
                  }))
                }
              />
            </label>
          </div>
          <div className="space-y-2 rounded-md border border-border/60 bg-surface-inset/25 p-3">
            <div className="flex items-center justify-between gap-2">
              <p className="text-xs font-medium text-foreground">
                非敏感环境变量
              </p>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={saving}
                onClick={addPlainEnvRow}
              >
                添加环境变量
              </Button>
            </div>
            {stdioConfig.envRows.length > 0 ? (
              <div className="space-y-2">
                {stdioConfig.envRows.map((row) => (
                  <div
                    key={row.id}
                    className="grid gap-2 md:grid-cols-[minmax(0,0.7fr)_minmax(0,1fr)_auto]"
                  >
                    <Input
                      value={row.name}
                      disabled={saving}
                      spellCheck={false}
                      placeholder={row.label ?? "变量名，例如 SEARXNG_URL"}
                      onChange={(event) =>
                        updatePlainEnvRow(row.id, { name: event.target.value })
                      }
                    />
                    <Input
                      value={row.value}
                      disabled={saving}
                      spellCheck={false}
                      placeholder={row.placeholder ?? "变量值"}
                      onChange={(event) =>
                        updatePlainEnvRow(row.id, { value: event.target.value })
                      }
                    />
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      disabled={saving}
                      onClick={() => removePlainEnvRow(row.id)}
                    >
                      移除
                    </Button>
                  </div>
                ))}
              </div>
            ) : null}
          </div>
        </section>
      ) : (
        <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_220px]">
          <label className="space-y-1 text-xs font-medium text-foreground">
            HTTPS 服务地址
            <Input
              value={httpsConfig.url}
              disabled={saving}
              spellCheck={false}
              placeholder="https://api.anysearch.com/mcp"
              onChange={(event) =>
                setHttpsConfig((current) => ({
                  ...current,
                  url: event.target.value,
                }))
              }
            />
          </label>
          <label className="flex items-center gap-2 self-end rounded-md border border-border/65 bg-card px-3 py-2 text-xs font-medium text-foreground">
            <input
              type="checkbox"
              className="h-4 w-4 rounded border-border accent-primary"
              checked={httpsConfig.allowLocalhostDev}
              disabled={saving}
              onChange={(event) =>
                setHttpsConfig((current) => ({
                  ...current,
                  allowLocalhostDev: event.target.checked,
                }))
              }
            />
            允许连接本机开发服务
          </label>
        </div>
      )}

      <div className="grid gap-3 md:grid-cols-2">
        <label className="space-y-1 text-xs font-medium text-foreground">
          搜索工具映射
          <Input
            value={searchTool}
            disabled={saving}
            spellCheck={false}
            placeholder="例如：search"
            onChange={(event) => setSearchTool(event.target.value)}
          />
        </label>
        <label className="space-y-1 text-xs font-medium text-foreground">
          网页读取工具映射
          <Input
            value={fetchTool}
            disabled={saving}
            spellCheck={false}
            placeholder="例如：extract"
            onChange={(event) => setFetchTool(event.target.value)}
          />
        </label>
      </div>

      <section className="space-y-2 rounded-md border border-border/60 bg-surface-inset/25 p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <p className="text-xs font-medium text-foreground">系统凭据引用</p>
            <p className="mt-0.5 text-[11px] text-muted-foreground">
              API Key 只写入本地加密凭据；Provider
              配置只保存引用名、请求头/环境变量名和 Bearer 方案。 当前状态：
              {credentialState}。
            </p>
          </div>
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={saving}
            onClick={addCredentialRow}
          >
            添加凭据引用
          </Button>
        </div>

        {credentialRows.length > 0 ? (
          <div className="space-y-2">
            {credentialRows.map((row) => (
              <div
                key={row.id}
                className="grid gap-2 md:grid-cols-[110px_minmax(0,0.7fr)_minmax(0,0.9fr)_minmax(0,1fr)_auto]"
              >
                <Select
                  value={row.target}
                  disabled={saving}
                  onValueChange={(value) =>
                    updateCredentialRow(row.id, {
                      target: value === "env" ? "env" : "header",
                    })
                  }
                >
                  <SelectTrigger className="h-9 text-xs">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="header">请求头</SelectItem>
                    <SelectItem value="env">环境变量</SelectItem>
                  </SelectContent>
                </Select>
                <Input
                  value={row.name}
                  disabled={saving}
                  spellCheck={false}
                  placeholder={
                    row.target === "header" ? "Authorization" : "BRAVE_API_KEY"
                  }
                  onChange={(event) =>
                    updateCredentialRow(row.id, { name: event.target.value })
                  }
                />
                <Input
                  value={row.ref}
                  disabled={saving}
                  spellCheck={false}
                  placeholder="iris.mcp.anysearch"
                  onChange={(event) =>
                    updateCredentialRow(row.id, { ref: event.target.value })
                  }
                />
                <Input
                  type="password"
                  value={row.secretValue}
                  disabled={saving}
                  spellCheck={false}
                  placeholder={
                    row.placeholder ??
                    `${row.label ?? "API Key"}${row.optional ? "（可选）" : ""}`
                  }
                  onChange={(event) =>
                    updateCredentialRow(row.id, {
                      secretValue: event.target.value,
                    })
                  }
                />
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={saving}
                  onClick={() => removeCredentialRow(row.id)}
                >
                  移除
                </Button>
              </div>
            ))}
          </div>
        ) : (
          <p className="text-xs text-muted-foreground">
            当前预设不需要 API Key；如服务侧要求鉴权，可添加请求头或环境变量
            凭据引用。
          </p>
        )}
      </section>

      {!persisted ? (
        <p className="text-xs text-muted-foreground">
          先保存提供方，再测试连接或查看诊断。
        </p>
      ) : null}

      {diagnosticLines.length > 0 ? (
        <div className="rounded-md border border-border/60 bg-surface-inset/40 px-3 py-2 text-xs text-muted-foreground">
          {diagnosticLines.map((check, index) => (
            <p key={`${provider.id}-${check.label}-${index}`}>
              {checkLabelText(check.label)}：{checkStatusText(check.status)} ·{" "}
              {check.message}
            </p>
          ))}
          <p>
            {hasLiveDiagnostics ? "实时可用性" : "配置可调度性"}：搜索
            {diagnostics?.canUseForSearch ? "可用" : "不可用"}
            ，网页读取{diagnostics?.canUseForFetch ? "可用" : "不可用"}
          </p>
        </div>
      ) : null}

      <div className="flex flex-wrap justify-end gap-2">
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={saving || !persisted}
          title={!persisted ? "请先保存提供方，再执行实时诊断" : undefined}
          onClick={() => void onDiagnostics(true)}
        >
          实时诊断
        </Button>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={saving}
          onClick={() => void onDelete()}
        >
          删除
        </Button>
        <Button type="button" size="sm" disabled={saving} onClick={handleSave}>
          保存 MCP 提供方
        </Button>
      </div>
    </article>
  );
}
