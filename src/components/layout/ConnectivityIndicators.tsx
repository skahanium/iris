import { memo } from "react";

import { cn } from "@/lib/utils";
import type { ConnectivityStatus, LlmConnectivityState } from "@/types/llm";

interface ConnectivityIndicatorsProps {
  status: ConnectivityStatus | null;
  onOpenSettings?: () => void;
}

function StatusDot({
  activeClass,
  inactiveClass,
  active,
  title,
  label,
  onClick,
}: {
  activeClass: string;
  inactiveClass: string;
  active: boolean;
  title: string;
  label: string;
  onClick?: () => void;
}) {
  return (
    <button
      type="button"
      className="group inline-flex h-6 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-md px-1.5 text-muted-foreground transition-[color,background-color,transform] duration-base ease-iris-out hover:bg-muted/60 focus:outline-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-1 focus-visible:ring-offset-panel"
      title={title}
      onClick={onClick}
    >
      <span
        className="relative flex size-3.5 shrink-0 items-center justify-center"
        aria-hidden
      >
        <span
          className={cn(
            "absolute inset-0 rounded-full border transition-[border-color,transform,opacity] duration-base ease-iris-out",
            active
              ? "scale-100 opacity-100"
              : "scale-[0.88] opacity-90 group-hover:scale-95",
            active ? activeClass : inactiveClass,
          )}
        />
        <span
          className={cn(
            "relative size-2 rounded-full transition-[transform,box-shadow] duration-base ease-iris-out",
            active ? "scale-110" : "scale-90 group-hover:scale-95",
            active ? activeClass : inactiveClass,
          )}
        />
      </span>
      <span className="hidden text-[11px] sm:inline">{label}</span>
    </button>
  );
}

function llmDotClasses(state: LlmConnectivityState): {
  active: string;
  inactive: string;
  on: boolean;
} {
  switch (state) {
    case "ready":
      return {
        on: true,
        active:
          "border-emerald-600/50 bg-gradient-to-br from-emerald-500 via-emerald-600 to-emerald-800 shadow-[0_0_0_1px_rgba(16,185,129,0.4),inset_0_1px_0_rgba(255,255,255,0.25)]",
        inactive: "",
      };
    case "missing_key":
      return {
        on: true,
        active:
          "border-amber-500/50 bg-gradient-to-br from-amber-400 via-amber-500 to-amber-700 shadow-[0_0_0_1px_rgba(245,158,11,0.4),inset_0_1px_0_rgba(255,255,255,0.22)]",
        inactive: "",
      };
    case "error":
      return {
        on: true,
        active:
          "border-destructive/50 bg-gradient-to-br from-red-500 via-red-600 to-red-800 shadow-[0_0_0_1px_rgba(239,68,68,0.4)]",
        inactive: "",
      };
    default:
      return {
        on: false,
        active: "",
        inactive:
          "border-muted-foreground/25 bg-gradient-to-br from-muted-foreground/50 to-muted-foreground/30",
      };
  }
}

export const ConnectivityIndicators = memo(function ConnectivityIndicators({
  status,
  onOpenSettings,
}: ConnectivityIndicatorsProps) {
  const llm = status?.llm;
  const search = status?.searchApi;
  const llmClasses = llmDotClasses(llm?.state ?? "misconfigured");
  const searchOn = search?.bingConfigured ?? false;

  const hit = status?.usageLast?.promptCacheHitTokens ?? 0;
  const miss = status?.usageLast?.promptCacheMissTokens ?? 0;
  const cachePct =
    hit + miss > 0 ? Math.round((hit / (hit + miss)) * 100) : null;

  const llmTitle = [
    llm?.message ?? "LLM 未检测",
    cachePct != null ? `上次缓存命中约 ${cachePct}%` : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <>
      <StatusDot
        label="LLM"
        title={llmTitle}
        active={llmClasses.on}
        activeClass={llmClasses.active}
        inactiveClass={llmClasses.inactive}
        onClick={onOpenSettings}
      />
      <StatusDot
        label="搜索 API"
        title={
          searchOn
            ? "Bing 搜索 API 已配置"
            : "未配置 Bing Key，使用 DuckDuckGo 降级"
        }
        active={searchOn}
        activeClass="border-teal-600/50 bg-gradient-to-br from-teal-500 via-teal-600 to-teal-800 shadow-[0_0_0_1px_rgba(20,184,166,0.4),inset_0_1px_0_rgba(255,255,255,0.25)]"
        inactiveClass="border-muted-foreground/25 bg-gradient-to-br from-muted-foreground/45 to-muted-foreground/28"
        onClick={onOpenSettings}
      />
    </>
  );
});
