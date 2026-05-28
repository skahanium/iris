import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

interface KbdProps {
  children: string;
  active?: boolean;
  className?: string;
}

/** 快捷键徽章（命令面板脚标、列表项右侧） */
export function Kbd({ children, active = false, className }: KbdProps) {
  return (
    <kbd
      className={cn(
        "shrink-0 rounded-md border px-1.5 py-0.5 font-sans text-[11px] leading-none transition-[color,background-color,border-color] duration-base ease-iris-out motion-reduce:transition-none",
        active
          ? "border-command-ring bg-command-highlight text-foreground/90"
          : "border-border/80 bg-surface-inset/80 text-muted-foreground",
        className,
      )}
    >
      {children}
    </kbd>
  );
}

interface OverlayFooterHintsProps {
  left: ReactNode;
  right?: ReactNode;
  className?: string;
}

/** 浮层底栏快捷键提示行 */
export function OverlayFooterHints({
  left,
  right,
  className,
}: OverlayFooterHintsProps) {
  return (
    <div
      className={cn(
        "flex h-10 shrink-0 items-center justify-between gap-3 border-t border-border bg-surface-inset/40 px-4 text-[11px] text-muted-foreground",
        className,
      )}
    >
      <span className="min-w-0 truncate">{left}</span>
      {right ? <span className="shrink-0">{right}</span> : null}
    </div>
  );
}
