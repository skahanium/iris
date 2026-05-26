import { X } from "lucide-react";
import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

const WIDTH_CLASS = {
  sm: "w-72",
  md: "w-80",
  lg: "w-96",
} as const;

export type SidePanelWidth = keyof typeof WIDTH_CLASS;

interface SidePanelProps {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
  width?: SidePanelWidth;
  /** AI 侧栏展开时，面板左移避免重叠与点击被挡 */
  aiPanelOpen?: boolean;
  className?: string;
  bodyClassName?: string;
}

export function SidePanel({
  open,
  onClose,
  title,
  children,
  width = "md",
  aiPanelOpen = false,
  className,
  bodyClassName,
}: SidePanelProps) {
  if (!open) return null;

  const edgeClass = aiPanelOpen ? "right-[280px]" : "right-0";

  return (
    <>
      <button
        type="button"
        aria-label="关闭面板"
        className={cn(
          "fixed inset-y-0 left-0 z-40 bg-foreground/15 backdrop-blur-[1px]",
          edgeClass,
        )}
        onClick={onClose}
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className={cn(
          "fixed inset-y-0 z-50 flex flex-col border-l border-border bg-panel shadow-2xl",
          WIDTH_CLASS[width],
          edgeClass,
          className,
        )}
      >
        <div className="flex h-10 shrink-0 items-center justify-between border-b border-border px-3">
          <span className="text-sm font-medium tracking-tight text-foreground">
            {title}
          </span>
          <Button
            type="button"
            size="icon"
            variant="ghost"
            className="h-8 w-8"
            onClick={onClose}
            aria-label="关闭"
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
        <div className={cn("flex min-h-0 flex-1 flex-col", bodyClassName)}>
          {children}
        </div>
      </div>
    </>
  );
}
