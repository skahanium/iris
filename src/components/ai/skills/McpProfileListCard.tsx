//! Compact list-card renderer for MCP evidence providers.
//!
//! Extracted from McpProfileCard so the card file only keeps the detail form.

import {
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Globe2,
  Terminal,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import type { WebEvidenceProviderSummary } from "@/lib/ipc";

import {
  MCP_PROVIDER_LIST_CARD_CLASS,
  mcpListDotAriaLabel,
  mcpListDotTone,
  mcpListDotClassName,
  mcpListMappingShortLabel,
  mcpListTransportShortLabel,
  type McpSearchRouteRole,
} from "./mcpProviderListUi";

export function McpProfileListCard({
  provider,
  searchRouteRole,
  saving,
  canMoveSearchRouteUp,
  canMoveSearchRouteDown,
  onSelect,
  onMoveSearchRoute,
}: {
  provider: WebEvidenceProviderSummary;
  searchRouteRole?: McpSearchRouteRole | null;
  saving: boolean;
  canMoveSearchRouteUp: boolean;
  canMoveSearchRouteDown: boolean;
  onSelect?: () => void;
  onMoveSearchRoute?: (delta: -1 | 1) => void;
}) {
  const listTransportKind = provider.transportKind as "stdio" | "https";
  const listDotTone = mcpListDotTone(provider);
  const TransportIcon = listTransportKind === "stdio" ? Terminal : Globe2;
  const searchRouteRoleLabel =
    searchRouteRole === "primary"
      ? "主服务"
      : searchRouteRole === "fallback_1"
        ? "备用 1"
        : searchRouteRole === "fallback_2"
          ? "备用 2"
          : null;
  const providerName = provider.name || "MCP 联网证据提供方";
  return (
    <div className="group relative">
      <button
        type="button"
        data-testid="mcp-provider-card"
        className={cn(MCP_PROVIDER_LIST_CARD_CLASS, "pr-24")}
        onClick={() => onSelect?.()}
      >
        <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-surface-inset/40">
          <TransportIcon
            className="h-4 w-4 text-muted-foreground"
            aria-hidden
          />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-2">
            <span
              className={cn(
                "inline-flex h-2 w-2 shrink-0 rounded-full",
                mcpListDotClassName(listDotTone),
              )}
              aria-label={mcpListDotAriaLabel(provider)}
            />
            <p className="truncate text-sm font-medium text-foreground">
              {providerName}
            </p>
            {searchRouteRoleLabel ? (
              <span className="shrink-0 rounded-full border border-success/25 bg-success-bg px-1.5 py-0.5 text-[10px] font-medium text-success-foreground">
                {searchRouteRoleLabel}
              </span>
            ) : null}
          </div>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {mcpListTransportShortLabel(listTransportKind)}
            {" · "}
            {mcpListMappingShortLabel(provider.mappingStatus)}
          </p>
        </div>
        <ChevronRight
          className="h-4 w-4 shrink-0 text-muted-foreground"
          aria-hidden
        />
      </button>
      {searchRouteRole && onMoveSearchRoute ? (
        <span className="absolute inset-y-0 right-8 z-10 flex items-center gap-0.5 opacity-0 transition-opacity duration-fast group-focus-within:opacity-100 group-hover:opacity-100 [@media(pointer:coarse)]:opacity-100">
          <Tooltip content="上移">
            <Button
              type="button"
              size="icon"
              variant="ghost"
              className="h-7 min-h-7 w-7 min-w-7"
              aria-label={`将 ${providerName} 上移`}
              disabled={saving || !canMoveSearchRouteUp}
              onClick={(event) => {
                event.stopPropagation();
                onMoveSearchRoute(-1);
              }}
            >
              <ChevronUp className="h-3.5 w-3.5" aria-hidden />
            </Button>
          </Tooltip>
          <Tooltip content="下移">
            <Button
              type="button"
              size="icon"
              variant="ghost"
              className="h-7 min-h-7 w-7 min-w-7"
              aria-label={`将 ${providerName} 下移`}
              disabled={saving || !canMoveSearchRouteDown}
              onClick={(event) => {
                event.stopPropagation();
                onMoveSearchRoute(1);
              }}
            >
              <ChevronDown className="h-3.5 w-3.5" aria-hidden />
            </Button>
          </Tooltip>
        </span>
      ) : null}
    </div>
  );
}
