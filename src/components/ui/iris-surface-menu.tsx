import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

export interface IrisSurfaceMenuItemProps {
  id: string;
  label: string;
  subtitle?: string;
  icon?: ReactNode;
  active?: boolean;
  disabled?: boolean;
  hint?: boolean;
  onSelect: () => void;
  onMouseEnter?: () => void;
  buttonRef?: (el: HTMLButtonElement | null) => void;
}

/** 编辑区 `/`、右键、@ 提及共用的浮层菜单行 */
export function IrisSurfaceMenuItem({
  id,
  label,
  subtitle,
  icon,
  active = false,
  disabled = false,
  hint = false,
  onSelect,
  onMouseEnter,
  buttonRef,
}: IrisSurfaceMenuItemProps) {
  if (hint) {
    return (
      <p
        id={id}
        className="px-3 py-2 text-[11px] leading-snug text-muted-foreground"
        role="presentation"
      >
        {label}
      </p>
    );
  }

  return (
    <button
      ref={buttonRef}
      type="button"
      id={id}
      role="menuitem"
      disabled={disabled}
      className={cn(
        "flex min-h-8 w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] leading-snug transition-colors duration-150",
        "hover:bg-[hsl(var(--command-highlight-bg))] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[hsl(var(--command-highlight-ring))]",
        active &&
          "bg-[hsl(var(--command-highlight-bg))] ring-1 ring-inset ring-[hsl(var(--command-highlight-ring))]",
        disabled && "pointer-events-none opacity-40",
      )}
      onMouseEnter={onMouseEnter}
      onClick={() => {
        if (disabled) return;
        onSelect();
      }}
    >
      {icon ? (
        <span className="flex h-4 w-4 shrink-0 items-center justify-center text-muted-foreground [&_svg]:h-4 [&_svg]:w-4">
          {icon}
        </span>
      ) : null}
      <span className="min-w-0 flex-1">
        <span className="block truncate font-medium text-foreground">
          {label}
        </span>
        {subtitle ? (
          <span className="block truncate text-[11px] font-normal text-muted-foreground">
            {subtitle}
          </span>
        ) : null}
      </span>
    </button>
  );
}

export function IrisSurfaceMenuGroup({
  title,
  children,
  className,
}: {
  title?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section role="presentation" className={className}>
      {title ? (
        <p className="px-3 pb-0.5 pt-2 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
          {title}
        </p>
      ) : null}
      {children}
    </section>
  );
}

export function IrisSurfaceMenuPanel({
  children,
  className,
  role = "menu",
  "aria-label": ariaLabel,
}: {
  children: ReactNode;
  className?: string;
  role?: "menu" | "listbox";
  "aria-label"?: string;
}) {
  return (
    <div
      role={role}
      aria-label={ariaLabel}
      className={cn(
        "overflow-auto rounded-lg border border-border/60 bg-[hsl(var(--surface-elevated))] py-1 shadow-floating",
        "motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-bottom-1 motion-safe:duration-150",
        className,
      )}
    >
      {children}
    </div>
  );
}
