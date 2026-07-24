import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

interface SurfaceCardProps {
  children: ReactNode;
  className?: string;
  selected?: boolean;
  onClick?: () => void;
  as?: "div" | "button";
}

/** 证据卡 / 工具卡统一表面 */
export function SurfaceCard({
  children,
  className,
  selected = false,
  onClick,
  as: Tag = onClick ? "button" : "div",
}: SurfaceCardProps) {
  return (
    <Tag
      type={Tag === "button" ? "button" : undefined}
      className={cn(
        "w-full rounded-lg border border-border/80 bg-surface-elevated p-2.5 text-left text-xs transition-[background-color,border-color] duration-base ease-iris-out motion-reduce:transition-none",
        selected && "border-primary/40 ring-2 ring-primary/30",
        onClick && "cursor-pointer hover:bg-surface-inset/60",
        className,
      )}
      onClick={onClick}
    >
      {children}
    </Tag>
  );
}
