import mcpSearchResultLimitManifest from "../../../../config/mcp-search-result-limit-manifest.json";
import { MCP_PROVIDER_PRESETS } from "./mcpProviderPresets";

export interface McpSearchResultLimitHealTarget {
  presetId: string;
  hosts: string[];
  providerName: string;
  maxResultsArg: string;
}

export const MCP_SEARCH_RESULT_LIMIT_HEAL_TARGETS: McpSearchResultLimitHealTarget[] =
  mcpSearchResultLimitManifest as McpSearchResultLimitHealTarget[];

function parseTransportConfig(
  transportConfigJson: string,
): Record<string, unknown> {
  try {
    const parsed: unknown = JSON.parse(transportConfigJson);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : {};
  } catch {
    return {};
  }
}

function hostFromUrl(url: string): string | null {
  try {
    return new URL(url).hostname.toLowerCase();
  } catch {
    return null;
  }
}

export function resolveSearchResultLimitHealTarget(provider: {
  name: string;
  transportConfigJson: string;
  presetId?: string | null;
}): McpSearchResultLimitHealTarget | null {
  const config = parseTransportConfig(provider.transportConfigJson);
  const presetId =
    provider.presetId?.trim() ||
    (typeof config.preset_id === "string" ? config.preset_id.trim() : "");
  if (presetId) {
    const byPreset = MCP_SEARCH_RESULT_LIMIT_HEAL_TARGETS.find(
      (target) => target.presetId === presetId,
    );
    if (byPreset) return byPreset;
  }

  const url = typeof config.url === "string" ? config.url : "";
  const host = url ? hostFromUrl(url) : null;
  if (host) {
    const byHost = MCP_SEARCH_RESULT_LIMIT_HEAL_TARGETS.find((target) =>
      target.hosts.some((item) => item.toLowerCase() === host),
    );
    if (byHost) return byHost;
  }

  const normalizedName = provider.name.trim().toLowerCase();
  return (
    MCP_SEARCH_RESULT_LIMIT_HEAL_TARGETS.find(
      (target) => target.providerName.trim().toLowerCase() === normalizedName,
    ) ?? null
  );
}

export function mappingHasMaxResultsArgument(
  raw: string | null | undefined,
): boolean {
  if (!raw?.trim()) return false;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return false;
    }
    const value = (parsed as Record<string, unknown>).maxResultsArg;
    return typeof value === "string" && value.trim().length > 0;
  } catch {
    return false;
  }
}

function mappingHasSearchTool(raw: string): boolean {
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return false;
    }
    const record = parsed as Record<string, unknown>;
    const tool =
      typeof record.tool === "string"
        ? record.tool
        : typeof record.tool_name === "string"
          ? record.tool_name
          : "";
    return tool.trim().length > 0;
  } catch {
    return false;
  }
}

/**
 * Persist search mappings with the preset-declared `maxResultsArg` when missing.
 */
export function ensureSearchMappingResultLimit(
  mappingJson: string | null | undefined,
  maxResultsArg: string,
): string | null {
  if (mappingJson == null) return null;
  const trimmed = mappingJson.trim();
  if (!trimmed) return null;
  if (!mappingHasSearchTool(trimmed)) return trimmed;
  if (mappingHasMaxResultsArgument(trimmed)) return trimmed;

  try {
    const parsed: unknown = JSON.parse(trimmed);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return trimmed;
    }
    const record = { ...(parsed as Record<string, unknown>) };
    record.maxResultsArg = maxResultsArg;
    return JSON.stringify(record);
  } catch {
    return trimmed;
  }
}

export function ensureProviderSearchMappingResultLimit(
  provider: {
    name: string;
    transportConfigJson: string;
    presetId?: string | null;
  },
  mappingJson: string | null | undefined,
): string | null {
  const target = resolveSearchResultLimitHealTarget(provider);
  if (!target) return mappingJson ?? null;
  return ensureSearchMappingResultLimit(mappingJson, target.maxResultsArg);
}

export function needsSearchResultLimitUpdate(provider: {
  name: string;
  transportConfigJson: string;
  presetId?: string | null;
  hasSearchMapping: boolean;
  searchMapping?: string | null;
}): boolean {
  if (!provider.hasSearchMapping) return false;
  const target = resolveSearchResultLimitHealTarget(provider);
  if (!target) return false;
  return !mappingHasMaxResultsArgument(provider.searchMapping);
}

/** Contract helper: presets with maxResultsArg must appear in the shared manifest. */
export function presetDeclaredResultLimitTargets(): McpSearchResultLimitHealTarget[] {
  return MCP_PROVIDER_PRESETS.flatMap((preset) => {
    if (!preset.searchMapping) return [];
    try {
      const parsed: unknown = JSON.parse(preset.searchMapping);
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        return [];
      }
      const maxResultsArg = (parsed as Record<string, unknown>).maxResultsArg;
      if (typeof maxResultsArg !== "string" || !maxResultsArg.trim()) {
        return [];
      }
      const host =
        typeof preset.url === "string" ? hostFromUrl(preset.url) : null;
      return [
        {
          presetId: preset.id,
          hosts: host ? [host] : [],
          providerName: preset.providerName,
          maxResultsArg,
        },
      ];
    } catch {
      return [];
    }
  });
}
