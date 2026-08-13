import * as DialogPrimitive from "@radix-ui/react-dialog";
import * as React from "react";

import { DialogOverlay, DialogPortal } from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

const Sheet = DialogPrimitive.Root;
const SheetClose = DialogPrimitive.Close;

type SheetContentProps = React.ComponentPropsWithoutRef<
  typeof DialogPrimitive.Content
> & {
  /** 让应用工作区抽屉避开桌面标题栏与 macOS 窗口控制区。 */
  topInset?: "none" | "titlebar";
};

function SheetContent({
  className,
  children,
  topInset = "none",
  ...props
}: SheetContentProps) {
  const belowTitlebar = topInset === "titlebar";
  return (
    <DialogPortal>
      <DialogOverlay
        className={belowTitlebar ? "top-[var(--titlebar-height)]" : undefined}
      />
      <DialogPrimitive.Content
        className={cn(
          "fixed left-0 z-overlay flex w-[min(18rem,calc(100vw-2rem))] flex-col border-r border-border-subtle bg-panel shadow-overlay outline-none",
          belowTitlebar ? "bottom-0 top-[var(--titlebar-height)]" : "inset-y-0",
          className,
        )}
        {...props}
      >
        <DialogPrimitive.Title className="sr-only">
          侧边面板
        </DialogPrimitive.Title>
        <DialogPrimitive.Description className="sr-only">
          可按 Escape 或点击遮罩关闭。
        </DialogPrimitive.Description>
        {children}
      </DialogPrimitive.Content>
    </DialogPortal>
  );
}

export { Sheet, SheetClose, SheetContent };
