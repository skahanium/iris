import * as ScrollAreaPrimitive from "@radix-ui/react-scroll-area";
import * as React from "react";

import { cn } from "@/lib/utils";

export function ScrollArea({
  className,
  children,
  viewportRef,
  scrollbarVisibility = "hover",
  ...props
}: React.ComponentProps<typeof ScrollAreaPrimitive.Root> & {
  viewportRef?: React.Ref<HTMLDivElement>;
  /** `always`：常显半透明滑块，便于长列表定位 */
  scrollbarVisibility?: "hover" | "scroll" | "auto" | "always";
}) {
  return (
    <ScrollAreaPrimitive.Root
      type={scrollbarVisibility}
      className={cn("relative overflow-hidden", className)}
      {...props}
    >
      <ScrollAreaPrimitive.Viewport
        ref={viewportRef}
        className="h-full w-full rounded-[inherit]"
      >
        {children}
      </ScrollAreaPrimitive.Viewport>
      <ScrollAreaPrimitive.Scrollbar
        orientation="vertical"
        className={cn(
          "flex w-2.5 touch-none select-none p-1",
          scrollbarVisibility === "always" && "opacity-100",
        )}
      >
        <ScrollAreaPrimitive.Thumb className="relative min-h-[2.5rem] flex-1 rounded-full bg-foreground/25 transition-colors hover:bg-foreground/40" />
      </ScrollAreaPrimitive.Scrollbar>
    </ScrollAreaPrimitive.Root>
  );
}
