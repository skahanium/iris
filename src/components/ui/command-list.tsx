import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

import { Kbd } from "@/components/ui/kbd";
import { cn } from "@/lib/utils";

interface CommandListGroupProps {
  title: string;
  className?: string;
}

export function CommandListGroup({ title, className }: CommandListGroupProps) {
  return (
    <p
      className={cn(
        "px-4 pb-1 pt-2 text-[11px] font-medium tracking-wider text-muted-foreground",
        className,
      )}
      style={{ color: "hsl(var(--command-group-label))" }}
    >
      {title}
    </p>
  );
}

interface LabelSegment {
  text: string;
  highlighted: boolean;
}

function splitLabelByMatch(label: string, query: string): LabelSegment[] {
  const q = query.trim().toLowerCase();
  if (!q) return [{ text: label, highlighted: false }];
  const idx = label.toLowerCase().indexOf(q);
  if (idx < 0) return [{ text: label, highlighted: false }];
  const end = idx + q.length;
  return [
    ...(idx > 0 ? [{ text: label.slice(0, idx), highlighted: false }] : []),
    { text: label.slice(idx, end), highlighted: true },
    ...(end < label.length
      ? [{ text: label.slice(end), highlighted: false }]
      : []),
  ];
}

interface HighlightedLabelProps {
  label: string;
  query?: string;
}

export function HighlightedLabel({ label, query = "" }: HighlightedLabelProps) {
  const segments = splitLabelByMatch(label, query);
  return (
    <span className="min-w-0 truncate">
      {segments.map((seg, i) =>
        seg.highlighted ? (
          <mark
            key={i}
            className="rounded-sm bg-command-highlight font-medium text-foreground"
          >
            {seg.text}
          </mark>
        ) : (
          <span key={i}>{seg.text}</span>
        ),
      )}
    </span>
  );
}

export interface CommandListOptionProps {
  id: string;
  label: string;
  query?: string;
  active?: boolean;
  disabled?: boolean;
  shortcut?: string;
  icon?: LucideIcon | null;
  subtitle?: string;
  onSelect?: () => void;
  onMouseEnter?: () => void;
  buttonRef?: (el: HTMLButtonElement | null) => void;
  className?: string;
  children?: ReactNode;
}

export function CommandListOption({
  id,
  label,
  query = "",
  active = false,
  disabled = false,
  shortcut,
  icon: Icon,
  subtitle,
  onSelect,
  onMouseEnter,
  buttonRef,
  className,
  children,
}: CommandListOptionProps) {
  return (
    <div className={cn("px-2.5 py-0.5", className)}>
      <button
        id={id}
        ref={buttonRef}
        type="button"
        role="option"
        aria-disabled={disabled || undefined}
        aria-selected={active}
        className={cn(
          "relative flex w-full scroll-my-2 items-center gap-3 rounded-lg px-3 py-2.5 text-left text-[15px] leading-snug",
          "transition-[background-color,box-shadow,color] duration-base ease-iris-out motion-reduce:transition-none",
          disabled && !active && "text-muted-foreground/75",
          active && !disabled
            ? "z-[1] bg-command-highlight font-medium text-foreground shadow-[inset_0_0_0_1px_hsl(var(--command-highlight-ring))]"
            : active && disabled
              ? "z-[1] bg-muted text-muted-foreground shadow-[inset_0_0_0_1px_hsl(var(--border))]"
              : "bg-transparent text-foreground/85",
          !active && !disabled && "hover:bg-surface-inset/80",
        )}
        onMouseEnter={onMouseEnter}
        onClick={() => {
          if (disabled) return;
          onSelect?.();
        }}
      >
        <span
          className={cn(
            "absolute bottom-2 left-2 top-2 w-0.5 rounded-full transition-opacity duration-base ease-iris-out motion-reduce:transition-none",
            active && !disabled
              ? "bg-primary opacity-100"
              : active && disabled
                ? "bg-muted-foreground/50 opacity-100"
                : "bg-primary opacity-0",
          )}
          aria-hidden
        />
        {Icon ? (
          <Icon
            className={cn(
              "h-4 w-4 shrink-0",
              active ? "text-foreground" : "text-muted-foreground",
            )}
            strokeWidth={1.75}
            aria-hidden
          />
        ) : null}
        <span className="min-w-0 flex-1 pl-0.5">
          {children ?? <HighlightedLabel label={label} query={query} />}
          {subtitle ? (
            <span className="mt-0.5 block truncate text-xs text-muted-foreground">
              {subtitle}
            </span>
          ) : null}
        </span>
        {shortcut ? <Kbd active={active && !disabled}>{shortcut}</Kbd> : null}
      </button>
    </div>
  );
}
