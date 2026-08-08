//! Shared presentation primitives for the Management Center.
//!
//! Extracted from ManagementCenterPanel so the panel stays a thin orchestrator.

import type { ReactNode } from "react";

import { cn } from "@/lib/utils";
import { ChevronLeft } from "lucide-react";

import { Button } from "@/components/ui/button";

import type { LucideIcon } from "lucide-react";

export type AiManagementDetail =
  | "models"
  | "web-search"
  | "persona"
  | "skills"
  | "memory";

export type NotesManagementDetail = "file-sheet" | "recycle-bin";

export function SectionShell({
  title,
  detail,
  children,
}: {
  title: string;
  detail: string;
  children: ReactNode;
}) {
  return (
    <section className="space-y-5">
      <header className="border-b border-border/60 pb-3">
        <h3 className="text-sm font-semibold text-foreground">{title}</h3>
        <p className="mt-1 text-xs text-muted-foreground">{detail}</p>
      </header>
      {children}
    </section>
  );
}

export function PanelSection({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <div className="space-y-2">
      <h4 className="text-xs font-semibold text-muted-foreground">{title}</h4>
      <div className="overflow-hidden rounded-lg border border-border/65 bg-background/55">
        {children}
      </div>
    </div>
  );
}

export function SettingRow({
  icon: Icon,
  title,
  detail,
  children,
}: {
  icon?: LucideIcon;
  title: string;
  detail?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="grid gap-3 border-b border-border/50 px-4 py-3 last:border-b-0 md:grid-cols-[minmax(12rem,1fr)_auto] md:items-center">
      <div className="flex min-w-0 gap-3">
        {Icon ? (
          <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-surface-inset text-muted-foreground">
            <Icon className="h-4 w-4" />
          </span>
        ) : null}
        <div className="min-w-0">
          <p className="text-sm font-medium text-foreground">{title}</p>
          {detail ? (
            <div className="mt-1 text-xs leading-relaxed text-muted-foreground">
              {detail}
            </div>
          ) : null}
        </div>
      </div>
      {children ? (
        <div className="flex items-center gap-2">{children}</div>
      ) : null}
    </div>
  );
}

export function StatusValue({
  ready,
  children,
}: {
  ready?: boolean;
  children: ReactNode;
}) {
  return (
    <span className="inline-flex items-center gap-2 rounded-md border border-border/50 bg-surface-inset/45 px-2.5 py-1 text-xs text-foreground">
      {typeof ready === "boolean" ? (
        <span
          className={cn(
            "size-2 rounded-full",
            ready
              ? "bg-[hsl(var(--status-llm-ready))]"
              : "bg-[hsl(var(--status-inactive)/0.65)]",
          )}
          aria-hidden
        />
      ) : null}
      {children}
    </span>
  );
}

export function SwitchControl({
  checked,
  onCheckedChange,
  label,
  disabled = false,
  "data-testid": dataTestId,
}: {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
  "data-testid"?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      data-testid={dataTestId}
      className={cn(
        "relative inline-flex h-7 w-12 shrink-0 overflow-hidden rounded-full border p-0 transition-colors duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/45 focus-visible:ring-offset-2 focus-visible:ring-offset-background",
        checked
          ? "border-[hsl(var(--status-llm-ready)/0.72)] bg-[hsl(var(--status-llm-ready))] shadow-[inset_0_1px_0_hsl(0_0%_100%/0.20),0_0_0_1px_hsl(var(--status-llm-ready)/0.12)]"
          : "border-border/70 bg-surface-inset shadow-inner",
        disabled && "cursor-not-allowed opacity-55",
      )}
      onClick={() => {
        if (!disabled) onCheckedChange(!checked);
      }}
    >
      <span
        className={cn(
          "pointer-events-none absolute left-1 top-1 size-5 rounded-full bg-white shadow-[0_1px_2px_hsl(0_0%_0%/0.24),0_0_0_1px_hsl(0_0%_0%/0.06)] ring-1 ring-black/5 transition-transform duration-200 ease-out",
          checked ? "translate-x-5" : "translate-x-0",
        )}
      />
    </button>
  );
}

export function DetailChrome({
  backLabel,
  onBack,
  title,
  detail,
  providerDetailActive,
}: {
  backLabel: string;
  onBack: () => void;
  title: string;
  detail: string;
  providerDetailActive?: boolean;
}) {
  return (
    <header
      className="relative flex items-center border-b border-border-subtle pb-3"
      data-management-provider-detail={
        providerDetailActive ? "true" : undefined
      }
    >
      <Button
        type="button"
        variant="ghost"
        size="sm"
        data-testid="management-detail-back"
        aria-label={`返回 ${backLabel}`}
        className="h-8 gap-1 rounded-full border border-border-subtle bg-surface-inset/40 px-3 text-xs text-muted-foreground hover:bg-surface-inset hover:text-foreground"
        onClick={onBack}
      >
        <ChevronLeft className="h-3.5 w-3.5" />
        {backLabel}
      </Button>
      <div className="pointer-events-none absolute inset-x-0 flex flex-col items-center text-center">
        <h3 className="text-sm font-semibold text-foreground">{title}</h3>
        <p className="mt-1 max-w-prose px-10 text-xs text-muted-foreground">
          {detail}
        </p>
      </div>
    </header>
  );
}
