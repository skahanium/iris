import type { WebEvidenceProviderSummary } from "@/lib/ipc";

export type McpListDotTone = "muted" | "success" | "warning";

export type McpSearchRouteRole = "primary" | "fallback_1" | "fallback_2";

interface SearchRouteEligibleProvider {
  id: string;
  enabled: boolean;
  hasSearchMapping: boolean;
}

export interface McpSearchRouteListItem<T> {
  provider: T;
  searchRouteRole?: McpSearchRouteRole;
}

/**
 * Projects MCP providers into the single list order used for web-search
 * priority. Only the first three enabled search mappings are route candidates.
 */
export function orderMcpProvidersForSearchRoute<
  T extends SearchRouteEligibleProvider,
>(
  providers: readonly T[],
  searchRouteIds: readonly string[],
): McpSearchRouteListItem<T>[] {
  const eligible = providers.filter(
    (provider) => provider.enabled && provider.hasSearchMapping,
  );
  const eligibleById = new Map(
    eligible.map((provider) => [provider.id, provider]),
  );
  const orderedCandidates: T[] = [];
  const candidateIds = new Set<string>();

  for (const id of searchRouteIds) {
    const provider = eligibleById.get(id);
    if (!provider || candidateIds.has(id)) continue;
    orderedCandidates.push(provider);
    candidateIds.add(id);
  }
  for (const provider of eligible) {
    if (candidateIds.has(provider.id)) continue;
    orderedCandidates.push(provider);
    candidateIds.add(provider.id);
  }

  const routeCandidates = orderedCandidates.slice(0, 3);
  const routeCandidateIds = new Set(
    routeCandidates.map((provider) => provider.id),
  );
  const roles: McpSearchRouteRole[] = ["primary", "fallback_1", "fallback_2"];

  return [
    ...routeCandidates.map((provider, index) => ({
      provider,
      searchRouteRole: roles[index],
    })),
    ...providers
      .filter((provider) => !routeCandidateIds.has(provider.id))
      .map((provider) => ({ provider })),
  ];
}

export function mcpListDotTone(
  provider: Pick<WebEvidenceProviderSummary, "enabled" | "mappingStatus">,
): McpListDotTone {
  if (!provider.enabled) {
    return "muted";
  }
  if (provider.mappingStatus === "complete") {
    return "success";
  }
  return "warning";
}

export function mcpListDotClassName(tone: McpListDotTone): string {
  switch (tone) {
    case "success":
      return "bg-success";
    case "warning":
      return "bg-amber-500";
    default:
      return "bg-muted-foreground/60";
  }
}

export function mcpListDotAriaLabel(
  provider: Pick<WebEvidenceProviderSummary, "enabled" | "mappingStatus">,
): string {
  if (!provider.enabled) {
    return "未启用";
  }
  if (provider.mappingStatus === "complete") {
    return "已启用，映射完整";
  }
  if (provider.mappingStatus === "partial") {
    return "已启用，映射不完整";
  }
  return "已启用，映射缺失";
}

export function mcpListMappingShortLabel(mappingStatus: string): string {
  switch (mappingStatus) {
    case "complete":
      return "映射完整";
    case "partial":
      return "映射不完整";
    case "missing":
      return "映射缺失";
    default:
      return mappingStatus;
  }
}

export function mcpListTransportShortLabel(transportKind: string): string {
  return transportKind === "stdio" ? "stdio" : "HTTPS";
}

export const MCP_PROVIDER_LIST_CARD_CLASS =
  "flex w-full items-center gap-3 rounded-lg border border-border/65 bg-background/55 p-3 text-left transition-colors hover:bg-muted/30";
