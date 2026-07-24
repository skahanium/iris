import * as DialogPrimitive from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import type { ReactNode } from "react";

import {
  irisOverlayPanelClass,
  type IrisOverlaySize,
} from "@/lib/overlay-sizes";
import { cn } from "@/lib/utils";

interface IrisOverlayProps {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
  size?: IrisOverlaySize;
  /** 默认 true；命令面板 / Quick Open 等自管搜索顶栏时设为 false */
  showTitleBar?: boolean;
  className?: string;
  bodyClassName?: string;
}

export function IrisOverlay({
  open,
  onClose,
  title,
  children,
  size = "command",
  showTitleBar = true,
  className,
  bodyClassName,
}: IrisOverlayProps) {
  return (
    <DialogPrimitive.Root
      open={open}
      onOpenChange={(next) => !next && onClose()}
    >
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay
          data-slot="iris-overlay-scrim"
          className="fixed inset-0 z-overlay-scrim bg-overlay-scrim backdrop-blur-[2px] duration-fast ease-iris-out data-[state=closed]:animate-iris-fade-out data-[state=open]:animate-iris-fade-in motion-reduce:data-[state=closed]:animate-none motion-reduce:data-[state=open]:animate-none"
          onClick={onClose}
        />
        <DialogPrimitive.Content
          aria-label={title}
          aria-describedby={undefined}
          className={irisOverlayPanelClass(size, cn("task-overlay", className))}
        >
          {showTitleBar ? (
            <div className="task-overlay-header flex h-11 shrink-0 items-center justify-between border-b border-border/60 bg-surface-elevated px-4">
              <DialogPrimitive.Title className="text-sm font-semibold tracking-tight text-foreground">
                {title}
              </DialogPrimitive.Title>
              <DialogPrimitive.Close
                className="iris-focus-soft inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground duration-fast ease-iris-out hover:bg-surface-inset hover:text-foreground focus:outline-none"
                aria-label="关闭"
              >
                <X className="h-4 w-4" />
              </DialogPrimitive.Close>
            </div>
          ) : (
            <DialogPrimitive.Title className="sr-only">
              {title}
            </DialogPrimitive.Title>
          )}
          <div className={cn("flex min-h-0 flex-1 flex-col", bodyClassName)}>
            {children}
          </div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
