import { memo } from "react";

import { cn } from "@/lib/utils";
import type { ConnectivityStatus, LlmConnectivityState } from "@/types/llm";

interface ConnectivityIndicatorsProps {
  status: ConnectivityStatus | null;
  onOpenSettings?: () => void;
  webSearch?: boolean;
  onWebSearchChange?: (enabled: boolean) => void;
}

interface StatusIndicatorProps {
  label: string;
  active: boolean;
  activeClass: string;
  title: string;
  ariaLabel: string;
  onClick?: () => void;
  role?: "button" | "switch";
  ariaChecked?: boolean;
}

/** 底栏统一规格：8px 圆点 + 简短文案，未就绪为灰，就绪为各通道 token 色 */
function StatusIndicator({
  label,
  active,
  activeClass,
  title,
  ariaLabel,
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
      className="iris-focus-soft inline-flex h-6 shrink-0 items-center gap-1 rounded-sm px-1.5 text-muted-foreground transition-[background-color,color,transform,box-shadow] duration-base ease-iris-out hover:bg-muted/50 focus:outline-none active:scale-[0.98]"
      onClick={onClick}
    >
      <span
        className={cn(
          "size-2 shrink-0 rounded-full transition-[background-color,box-shadow] duration-base ease-iris-out",
          active
            ? activeClass
            : "bg-[hsl(var(--status-inactive)/0.42)] shadow-[inset_0_1px_0_hsl(0_0%_100%/0.12)]",
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
      return "bg-[hsl(var(--status-llm-ready))] shadow-[0_0_0_1px_hsl(var(--status-llm-ready)/0.35)]";
    case "error":
      return "bg-[hsl(var(--status-llm-error))] shadow-[0_0_0_1px_hsl(var(--status-llm-error)/0.35)]";
    default:
      return "";
  }
}

function llmBallActive(state: LlmConnectivityState): boolean {
  return state === "ready" || state === "error";
}

export const ConnectivityIndicators = memo(function ConnectivityIndicators({
  status,
  onOpenSettings,
  webSearch = false,
  onWebSearchChange,
}: ConnectivityIndicatorsProps) {
  const llm = status?.llm;
  const llmState = llm?.state ?? "misconfigured";

  const hit = status?.usageLast?.promptCacheHitTokens ?? 0;
  const miss = status?.usageLast?.promptCacheMissTokens ?? 0;
  const cachePct =
    hit + miss > 0 ? Math.round((hit / (hit + miss)) * 100) : null;

  const search = status?.searchApi;
  const searchLabel = "MCP 优先 / DuckDuckGo 托底";

  const llmTitle = [
    llm?.message ?? "LLM 未检测",
    cachePct != null ? `上次缓存命中约 ${cachePct}%` : null,
    search ? `检索：${searchLabel}` : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <div
      className="inline-flex shrink-0 items-center gap-px rounded-md border border-border/40 bg-surface-inset/30 py-px pl-0.5 pr-1"
      role="group"
      aria-label="连通性"
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
          ariaChecked={webSearch}
          label="联网"
          active={webSearch}
          activeClass="bg-[hsl(var(--status-web-search))] shadow-[0_0_0_1px_hsl(var(--status-web-search)/0.35)]"
          title={
            webSearch
              ? `联网搜索：已开启 · 检索：${searchLabel}`
              : "联网搜索：已关闭"
          }
          ariaLabel={webSearch ? "关闭联网搜索" : "开启联网搜索"}
          onClick={() => onWebSearchChange(!webSearch)}
        />
      ) : null}
    </div>
  );
});
