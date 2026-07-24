import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import type { ComponentPropsWithoutRef, ReactNode } from "react";

import { cn } from "@/lib/utils";

const TooltipProvider = TooltipPrimitive.Provider;
const TooltipRoot = TooltipPrimitive.Root;
const TooltipTrigger = TooltipPrimitive.Trigger;

function TooltipContent({
  className,
  sideOffset = 6,
  ...props
}: ComponentPropsWithoutRef<typeof TooltipPrimitive.Content>) {
  return (
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Content
        sideOffset={sideOffset}
        className={cn(
          "z-overlay max-w-xs animate-iris-enter rounded-md border border-border bg-popover px-2 py-1 text-caption text-popover-foreground shadow-floating motion-reduce:animate-none",
          className,
        )}
        {...props}
      />
    </TooltipPrimitive.Portal>
  );
}

interface TooltipProps {
  content: ReactNode;
  children: ReactNode;
  side?: ComponentPropsWithoutRef<typeof TooltipPrimitive.Content>["side"];
  delayDuration?: number;
  /** When false, render children without tooltip chrome. */
  enabled?: boolean;
}

/**
 * Lightweight Iris tooltip. Prefer over native `title=` for icon-only chrome.
 */
export function Tooltip({
  content,
  children,
  side = "top",
  delayDuration = 300,
  enabled = true,
}: TooltipProps) {
  if (!enabled) return children;

  return (
    <TooltipProvider delayDuration={delayDuration}>
      <TooltipRoot>
        <TooltipTrigger asChild>{children}</TooltipTrigger>
        <TooltipContent side={side}>{content}</TooltipContent>
      </TooltipRoot>
    </TooltipProvider>
  );
}

export { TooltipContent, TooltipProvider, TooltipRoot, TooltipTrigger };
