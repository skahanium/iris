import {
  ensureAnySearchSearchMapping,
  isAnySearchTransportUrl,
  mappingHasMaxResultsArgument,
} from "./mcpAnySearchMapping";

export interface McpCredentialStateRow {
  ref: string;
  optional?: boolean;
  secretValue: string;
}

export function isAnySearchProvider(provider: {
  name: string;
  transportConfigJson: string;
}): boolean {
  if (provider.name.trim().toLowerCase() === "anysearch") return true;
  try {
    const parsed: unknown = JSON.parse(provider.transportConfigJson);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return false;
    }
    const url = (parsed as Record<string, unknown>).url;
    return typeof url === "string" && isAnySearchTransportUrl(url);
  } catch {
    return false;
  }
}

/** True when persisted AnySearch mapping still lacks maxResultsArg. */
export function needsAnySearchResultLimitUpdate(provider: {
  name: string;
  transportConfigJson: string;
  hasSearchMapping: boolean;
  searchMapping?: string | null;
}): boolean {
  return (
    isAnySearchProvider(provider) &&
    provider.hasSearchMapping &&
    !mappingHasMaxResultsArgument(provider.searchMapping)
  );
}

function credentialRowIsConfigured(
  row: McpCredentialStateRow,
  configuredByService?: Record<string, boolean>,
): boolean {
  const service = row.ref.trim().replace(/^credential:\/\//, "");
  if (!service) return false;
  return configuredByService?.[service] === true;
}

export function credentialStateText(
  rows: McpCredentialStateRow[],
  configuredByService?: Record<string, boolean>,
): string {
  if (rows.length === 0) return "不需要凭据";
  const hasPendingKey = rows.some((row) => row.secretValue.trim().length > 0);
  if (hasPendingKey) return "本次保存会更新 Key，请求将携带 Bearer";

  const configured = rows.filter((row) =>
    credentialRowIsConfigured(row, configuredByService),
  );
  const missingRequired = rows.filter(
    (row) =>
      row.optional !== true &&
      !credentialRowIsConfigured(row, configuredByService),
  );
  if (missingRequired.length > 0) {
    return "必填凭据缺失或待填写";
  }
  if (configured.length === rows.length) {
    return "已绑定 Key，请求将携带 Bearer";
  }
  if (configured.length > 0) {
    return "部分 Key 已绑定；未绑定的可选凭据将走匿名额度";
  }
  if (rows.every((row) => row.optional === true)) {
    return "未配置 Key，将使用匿名额度";
  }
  return "必填凭据缺失或待填写";
}

export function mappingForSave(
  raw: string,
  toolName: string,
  options?: { ensureAnySearchMaxResults?: boolean },
): string | null {
  const tool = toolName.trim();
  if (!tool) return null;
  let serialized: string;
  try {
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      serialized = JSON.stringify({ ...parsed, tool });
    } else {
      serialized = JSON.stringify({ tool });
    }
  } catch {
    serialized = JSON.stringify({ tool });
  }
  if (options?.ensureAnySearchMaxResults) {
    return ensureAnySearchSearchMapping(serialized) ?? serialized;
  }
  return serialized;
}
