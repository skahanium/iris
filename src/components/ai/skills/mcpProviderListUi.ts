import type { WebEvidenceProviderSummary } from "@/lib/ipc";

export type McpListDotTone = "muted" | "success" | "warning";

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
