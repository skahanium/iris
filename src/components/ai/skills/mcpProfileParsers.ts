//! Pure parsing/serialization helpers for MCP provider profiles.
//!
//! Extracted from McpProfileCard so the card stays a form orchestrator.

import type {
  DomainOperation,
  DomainOutputMapping,
  WebEvidenceProviderDiagnostics,
  WebEvidenceProviderInput,
  WebEvidenceProviderSummary,
} from "@/lib/ipc";

import type { McpProviderPreset, McpTransportKind } from "./mcpProviderPresets";
import type { McpSearchRouteRole } from "./mcpProviderListUi";

export interface McpCredentialSave {
  service: string;
  value: string;
}

export interface McpProfileCardProps {
  provider: WebEvidenceProviderSummary;
  diagnostics?: WebEvidenceProviderDiagnostics | null;
  /** service → whether a Key is already stored locally */
  credentialConfiguredByService?: Record<string, boolean>;
  saving?: boolean;
  persisted?: boolean;
  surface?: "list" | "detail";
  onSelect?: () => void;
  /** Visible web-search priority role when this provider is a route candidate. */
  searchRouteRole?: McpSearchRouteRole;
  canMoveSearchRouteUp?: boolean;
  canMoveSearchRouteDown?: boolean;
  onMoveSearchRoute?: (direction: -1 | 1) => void;
  onSave: (
    input: WebEvidenceProviderInput,
    credentialSaves: McpCredentialSave[],
  ) => void | Promise<void>;
  onToggle: (enabled: boolean) => void | Promise<void>;
  onDelete: () => void | Promise<void>;
  onClearCredential: (service: string) => void | Promise<void>;
  onDiagnostics: () => void | Promise<void>;
  onConfigurationChanged: () => void;
}

export interface DomainOperationOption {
  value: DomainOperation;
  label: string;
  group: string;
  requiredFields: string[];
  fieldLabels: Record<string, string>;
}

export const DOMAIN_OPERATION_OPTIONS: DomainOperationOption[] = [
  {
    value: "weather.current",
    label: "当前天气",
    group: "天气",
    requiredFields: ["location", "temperature", "observedAt", "sourceUrl"],
    fieldLabels: {
      location: "地点",
      temperature: "温度",
      observedAt: "观测时间",
      sourceUrl: "来源 URL",
    },
  },
  {
    value: "weather.forecast",
    label: "天气预报",
    group: "天气",
    requiredFields: ["location", "temperature", "issueAt", "sourceUrl"],
    fieldLabels: {
      location: "地点",
      temperature: "温度",
      issueAt: "发布时间",
      sourceUrl: "来源 URL",
    },
  },
  {
    value: "news.search",
    label: "新闻搜索",
    group: "新闻",
    requiredFields: ["title", "publishedAt", "sourceUrl"],
    fieldLabels: {
      title: "标题",
      publishedAt: "发布时间",
      sourceUrl: "来源 URL",
    },
  },
  {
    value: "finance.quote",
    label: "行情报价",
    group: "金融",
    requiredFields: ["instrument", "currency", "asOf", "sourceUrl"],
    fieldLabels: {
      instrument: "标的",
      currency: "币种",
      asOf: "数据时点",
      sourceUrl: "来源 URL",
    },
  },
  {
    value: "finance.metrics",
    label: "财务指标",
    group: "金融",
    requiredFields: ["instrument", "currency", "asOf", "sourceUrl"],
    fieldLabels: {
      instrument: "标的",
      currency: "币种",
      asOf: "数据时点",
      sourceUrl: "来源 URL",
    },
  },
  {
    value: "finance.news",
    label: "金融新闻",
    group: "金融",
    requiredFields: ["title", "publishedAt", "sourceUrl"],
    fieldLabels: {
      title: "标题",
      publishedAt: "发布时间",
      sourceUrl: "来源 URL",
    },
  },
  {
    value: "entertainment.now_playing",
    label: "正在上映",
    group: "影视",
    requiredFields: [
      "title",
      "region",
      "channel",
      "date",
      "checkedAt",
      "sourceUrl",
    ],
    fieldLabels: {
      title: "标题",
      region: "地区",
      channel: "频道/平台",
      date: "日期",
      checkedAt: "检查时间",
      sourceUrl: "来源 URL",
    },
  },
  {
    value: "entertainment.upcoming",
    label: "即将上映",
    group: "影视",
    requiredFields: [
      "title",
      "region",
      "channel",
      "date",
      "checkedAt",
      "sourceUrl",
    ],
    fieldLabels: {
      title: "标题",
      region: "地区",
      channel: "频道/平台",
      date: "日期",
      checkedAt: "检查时间",
      sourceUrl: "来源 URL",
    },
  },
  {
    value: "entertainment.streaming",
    label: "流媒体可看",
    group: "影视",
    requiredFields: [
      "title",
      "region",
      "channel",
      "date",
      "checkedAt",
      "sourceUrl",
    ],
    fieldLabels: {
      title: "标题",
      region: "地区",
      channel: "频道/平台",
      date: "日期",
      checkedAt: "检查时间",
      sourceUrl: "来源 URL",
    },
  },
  {
    value: "sports.schedule",
    label: "赛程",
    group: "体育",
    requiredFields: [
      "competition",
      "participants",
      "startTime",
      "checkedAt",
      "sourceUrl",
    ],
    fieldLabels: {
      competition: "赛事",
      participants: "参赛方",
      startTime: "开始时间",
      checkedAt: "检查时间",
      sourceUrl: "来源 URL",
    },
  },
  {
    value: "sports.score",
    label: "比分",
    group: "体育",
    requiredFields: [
      "competition",
      "participants",
      "startTime",
      "checkedAt",
      "sourceUrl",
    ],
    fieldLabels: {
      competition: "赛事",
      participants: "参赛方",
      startTime: "开始时间",
      checkedAt: "检查时间",
      sourceUrl: "来源 URL",
    },
  },
];

export function domainOperationMeta(
  operation: DomainOperation,
): DomainOperationOption {
  return (
    DOMAIN_OPERATION_OPTIONS.find((item) => item.value === operation) ??
    DOMAIN_OPERATION_OPTIONS[0]!
  );
}

/** 与 Rust 侧一致：仅允许 `$`、点属性、非负数组下标。 */
export function isSafeJsonPath(path: string): boolean {
  const value = path.trim();
  if (value === "$") return true;
  if (!value.startsWith("$")) return false;
  const rest = value.slice(1);
  let index = 0;
  while (index < rest.length) {
    const character = rest[index]!;
    if (character === ".") {
      index += 1;
      const start = index;
      while (
        index < rest.length &&
        rest[index] !== "." &&
        rest[index] !== "["
      ) {
        index += 1;
      }
      if (index === start) return false;
      const property = rest.slice(start, index);
      if (![...property].every((item) => /[A-Za-z0-9_-]/.test(item))) {
        return false;
      }
    } else if (character === "[") {
      const close = rest.indexOf("]", index);
      if (close < 0) return false;
      const digits = rest.slice(index + 1, close);
      if (
        digits.length === 0 ||
        ![...digits].every((item) => /\d/.test(item))
      ) {
        return false;
      }
      index = close + 1;
    } else {
      return false;
    }
  }
  return true;
}

/** 去除首尾空白并按字段名排序，与后端 BTreeMap 规范化保持一致。 */
export function normalizeOutputMapping(
  mapping: DomainOutputMapping,
): DomainOutputMapping {
  const recordsPath = mapping.recordsPath.trim();
  const fields = Object.fromEntries(
    Object.entries(mapping.fields)
      .map(([field, path]) => [field.trim(), path.trim()] as const)
      .filter(([field, path]) => field.length > 0 && path.length > 0)
      .sort(([left], [right]) => left.localeCompare(right)),
  );
  return { recordsPath, fields };
}

export function validateDomainMappingSave(input: {
  readOnly: boolean;
  riskClass: string;
  operation: DomainOperation | "";
  recordsPath: string;
  fields: Record<string, string>;
  existingOperations: DomainOperation[];
}): string | null {
  if (input.readOnly !== true || input.riskClass !== "read_only") {
    return "工具缺少只读声明或属于写操作，不能保存为当前事实映射。";
  }
  if (!input.operation) {
    return "请先选择当前事实操作。";
  }
  const meta = domainOperationMeta(input.operation);
  const missing = meta.requiredFields.filter(
    (field) => !input.fields[field]?.trim(),
  );
  if (missing.length > 0) {
    const labels = missing
      .map((field) => meta.fieldLabels[field] ?? field)
      .join("、");
    return `请补全必需字段映射：${labels}。`;
  }
  if (!isSafeJsonPath(input.recordsPath)) {
    return "记录路径不是受支持的 JSON path。";
  }
  for (const [field, path] of Object.entries(input.fields)) {
    if (!path.trim()) continue;
    if (!isSafeJsonPath(path)) {
      const label = meta.fieldLabels[field] ?? field;
      return `${label} 不是受支持的 JSON path。`;
    }
  }
  if (input.existingOperations.includes(input.operation)) {
    return `该提供方已存在 ${input.operation} 映射，请勿重复配置。`;
  }
  return null;
}

export interface HttpsConfigState {
  url: string;
  allowLocalhostDev: boolean;
}

export interface StdioConfigState {
  command: string;
  argsText: string;
  envRows: PlainEnvRow[];
}

export interface PlainEnvRow {
  id: string;
  name: string;
  value: string;
  label?: string;
  placeholder?: string;
}

export interface CredentialRefRow {
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
export function editableTransportKind(
  value: string | null | undefined,
): McpTransportKind {
  return value === "stdio" ? "stdio" : "https";
}

export function parseJsonRecord(
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

export function stringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value.map((item) => (typeof item === "string" ? item : String(item)))
    : [];
}

export function parseHttpsConfig(
  raw: string | null | undefined,
): HttpsConfigState {
  const parsed = parseJsonRecord(raw);
  return {
    url: typeof parsed.url === "string" ? parsed.url : "",
    allowLocalhostDev: parsed.allow_localhost_dev === true,
  };
}

export function parsePlainEnvRows(
  raw: string | null | undefined,
): PlainEnvRow[] {
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

export function parseStdioConfig(
  raw: string | null | undefined,
): StdioConfigState {
  const parsed = parseJsonRecord(raw);
  return {
    command: typeof parsed.command === "string" ? parsed.command : "",
    argsText: stringArray(parsed.args).join("\n"),
    envRows: parsePlainEnvRows(raw),
  };
}

export function credentialService(raw: unknown): string {
  if (typeof raw === "string") return raw.replace(/^credential:\/\//, "");
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return "";
  const record = raw as Record<string, unknown>;
  const service = record.credential ?? record.service ?? record.ref;
  return typeof service === "string"
    ? service.replace(/^credential:\/\//, "")
    : "";
}

export function credentialScheme(raw: unknown): string | undefined {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return undefined;
  const scheme = (raw as Record<string, unknown>).scheme;
  return typeof scheme === "string" && scheme.trim()
    ? scheme.trim()
    : undefined;
}

export function credentialOptional(raw: unknown): boolean {
  return (
    raw != null &&
    typeof raw === "object" &&
    !Array.isArray(raw) &&
    (raw as Record<string, unknown>).optional === true
  );
}

export function parseCredentialRows(
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

export function rowsFromPreset(preset: McpProviderPreset): CredentialRefRow[] {
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

export function plainEnvRowsFromPreset(
  preset: McpProviderPreset,
): PlainEnvRow[] {
  return preset.plainEnv.map((item, index) => ({
    id: `plain-env-${preset.id}-${index}-${item.name}`,
    name: item.name,
    value: item.value,
    label: item.label,
    placeholder: item.placeholder,
  }));
}

export function argsTextToArray(value: string): string[] {
  return value
    .split(/\r?\n/)
    .map((item) => item.trim())
    .filter(Boolean);
}

export function mappingToolName(
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

export function credentialRowsToJson(rows: CredentialRefRow[]): string {
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

export function plainEnvRowsToRecord(
  rows: PlainEnvRow[],
): Record<string, string> | undefined {
  const env = Object.fromEntries(
    rows
      .map((row) => [row.name.trim(), row.value.trim()] as const)
      .filter(([name, value]) => name.length > 0 && value.length > 0),
  );
  return Object.keys(env).length > 0 ? env : undefined;
}

export function statusText(enabled: boolean): string {
  return enabled ? "已启用" : "已停用";
}

export function transportLabel(kind: McpTransportKind): string {
  return kind === "stdio" ? "本地命令 (stdio)" : "HTTPS 服务";
}

export function mappingStatusText(status: string): string {
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

export function checkStatusText(status: string): string {
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

export function checkLabelText(label: string): string {
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
    case "circuit":
      return "熔断状态";
    case "searchToolLive":
      return "搜索工具";
    case "searchSmokeAuthHeader":
      return "鉴权请求头";
    case "authFingerprint":
      return "鉴权指纹";
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

export function presetIdFromProvider(
  provider: WebEvidenceProviderSummary,
): string {
  const parsed = parseJsonRecord(provider.transportConfigJson);
  return typeof parsed.preset_id === "string" ? parsed.preset_id : "custom";
}
