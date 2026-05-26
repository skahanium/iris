import * as DialogPrimitive from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import { useEffect, type ReactNode } from "react";

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
  className?: string;
  bodyClassName?: string;
}

export function IrisOverlay({
  open,
  onClose,
  title,
  children,
  size = "command",
  className,
  bodyClassName,
}: IrisOverlayProps) {
  useEffect(() => {
    if (!open) return;

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [open, onClose]);

  return (
    <DialogPrimitive.Root
      open={open}
      onOpenChange={(next) => !next && onClose()}
    >
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay
          data-slot="iris-overlay-scrim"
          className="data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 fixed inset-0 z-overlay-scrim bg-overlay-scrim backdrop-blur-[2px] duration-fast ease-iris-out"
          onClick={onClose}
        />
        <DialogPrimitive.Content
          aria-label={title}
          aria-describedby={undefined}
          className={irisOverlayPanelClass(size, className)}
        >
          <div className="flex h-11 shrink-0 items-center justify-between border-b border-border/60 px-4">
            <DialogPrimitive.Title className="text-sm font-medium tracking-tight text-foreground">
              {title}
            </DialogPrimitive.Title>
            <DialogPrimitive.Close
              className="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground duration-fast ease-iris-out hover:bg-muted hover:text-foreground focus:outline-none focus:ring-2 focus:ring-primary focus:ring-offset-1 focus:ring-offset-panel"
              aria-label="关闭"
            >
              <X className="h-4 w-4" />
            </DialogPrimitive.Close>
          </div>
          <div className={cn("flex min-h-0 flex-1 flex-col", bodyClassName)}>
            {children}
          </div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
