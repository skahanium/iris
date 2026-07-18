import {
  ensureProviderSearchMappingResultLimit,
  mappingHasMaxResultsArgument,
  needsSearchResultLimitUpdate,
  resolveSearchResultLimitHealTarget,
} from "./mcpSearchMappingHeal";

export interface McpCredentialStateRow {
  ref: string;
  optional?: boolean;
  secretValue: string;
}

export { needsSearchResultLimitUpdate, resolveSearchResultLimitHealTarget };

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
  provider?: {
    name: string;
    transportConfigJson: string;
    presetId?: string | null;
  },
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
  if (!provider) return serialized;
  return (
    ensureProviderSearchMappingResultLimit(provider, serialized) ?? serialized
  );
}

export function mappingNeedsResultLimitHeal(provider: {
  name: string;
  transportConfigJson: string;
  presetId?: string | null;
  searchMapping?: string | null;
}): boolean {
  const target = resolveSearchResultLimitHealTarget(provider);
  if (!target) return false;
  return !mappingHasMaxResultsArgument(provider.searchMapping);
}
