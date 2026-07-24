import { memo } from "react";

import { cn } from "@/lib/utils";
import {
  webSearchStatusDetail,
  type WebSearchAvailability,
} from "@/lib/web-search-provider-state";
import type { ConnectivityStatus, LlmConnectivityState } from "@/types/llm";

interface ConnectivityIndicatorsProps {
  status: ConnectivityStatus | null;
  onOpenSettings?: () => void;
  webSearch?: boolean;
  webSearchAvailability?: WebSearchAvailability | null;
  onWebSearchChange?: (enabled: boolean) => void;
}

interface StatusIndicatorProps {
  label: string;
  active: boolean;
  activeClass: string;
  title: string;
  ariaLabel: string;
  disabled?: boolean;
  onClick?: () => void;
  role?: "button" | "switch";
  ariaChecked?: boolean;
}

function StatusIndicator({
  label,
  active,
  activeClass,
  title,
  ariaLabel,
  disabled = false,
  onClick,
  role = "button",
  ariaChecked,
}: StatusIndicatorProps) {
  return (
    <button
      type="button"
      role={role}
      aria-checked={role === "switch" ? ariaChecked : undefined}
      aria-label={ariaLabel}
      title={title}
      disabled={disabled}
      className={cn(
        "iris-focus-soft inline-flex h-6 shrink-0 items-center gap-1 rounded-sm px-1.5 text-muted-foreground transition-[background-color,color,transform,box-shadow] duration-base ease-iris-out focus:outline-none active:scale-[0.98]",
        disabled
          ? "cursor-not-allowed opacity-55"
          : "hover:bg-muted/50 hover:text-foreground",
      )}
      onClick={disabled ? undefined : onClick}
    >
      <span
        className={cn(
          "size-2 shrink-0 rounded-full transition-[background-color,box-shadow] duration-base ease-iris-out",
          active
            ? activeClass
            : "bg-status-inactive/42 shadow-[inset_0_1px_0_hsl(0_0%_100%/0.12)]",
        )}
        aria-hidden
      />
      <span
        className={cn(
          "text-[10px] leading-none tracking-wide transition-colors duration-base ease-iris-out",
          active ? "text-foreground/80" : "text-muted-foreground",
        )}
      >
        {label}
      </span>
    </button>
  );
}

function llmBallActiveClass(state: LlmConnectivityState): string {
  switch (state) {
    case "ready":
      return "bg-status-llm-ready shadow-[0_0_0_1px_hsl(var(--status-llm-ready)/0.35)]";
    case "error":
      return "bg-status-llm-error shadow-[0_0_0_1px_hsl(var(--status-llm-error)/0.35)]";
    default:
      return "";
  }
}

function llmBallActive(state: LlmConnectivityState): boolean {
  return state === "ready" || state === "error";
}

function fallbackWebSearchDetail(webSearch: boolean): string {
  return webSearch ? "已开启" : "未开启";
}

export const ConnectivityIndicators = memo(function ConnectivityIndicators({
  status,
  onOpenSettings,
  webSearch = false,
  webSearchAvailability = null,
  onWebSearchChange,
}: ConnectivityIndicatorsProps) {
  const llm = status?.llm;
  const llmState = llm?.state ?? "misconfigured";

  const hit = status?.usageLast?.promptCacheHitTokens ?? 0;
  const miss = status?.usageLast?.promptCacheMissTokens ?? 0;
  const cachePct =
    hit + miss > 0 ? Math.round((hit / (hit + miss)) * 100) : null;

  const webDetail = webSearchAvailability
    ? webSearchStatusDetail(webSearch, webSearchAvailability)
    : fallbackWebSearchDetail(webSearch);
  const canToggleWebSearch = webSearchAvailability?.canEnable ?? true;
  const webSearchActive = webSearch && canToggleWebSearch;

  const llmTitle = [
    llm?.message ?? "LLM 未检测",
    cachePct != null ? `上次缓存命中约 ${cachePct}%` : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <div
      className="inline-flex shrink-0 items-center gap-px rounded-md border border-border/40 bg-surface-inset/30 py-px pl-0.5 pr-1"
      role="group"
      aria-label="连接状态"
    >
      <StatusIndicator
        label="LLM"
        active={llmBallActive(llmState)}
        activeClass={llmBallActiveClass(llmState)}
        title={llmTitle}
        ariaLabel={`LLM：${llm?.message ?? "未检测"}`}
        onClick={onOpenSettings}
      />
      {onWebSearchChange ? (
        <StatusIndicator
          role="switch"
          ariaChecked={webSearchActive}
          label="联网"
          active={webSearchActive}
          activeClass="bg-status-web-search shadow-[0_0_0_1px_hsl(var(--status-web-search)/0.35)]"
          title={`联网搜索：${webDetail}`}
          ariaLabel={webSearchActive ? "关闭联网搜索" : "开启联网搜索"}
          disabled={!canToggleWebSearch}
          onClick={() => onWebSearchChange(!webSearch)}
        />
      ) : null}
    </div>
  );
});
