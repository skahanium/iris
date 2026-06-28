import * as DialogPrimitive from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import * as React from "react";

import {
  irisOverlayPanelClass,
  type IrisOverlaySize,
} from "@/lib/overlay-sizes";
import { cn } from "@/lib/utils";

const Dialog = DialogPrimitive.Root;
const DialogTrigger = DialogPrimitive.Trigger;
const DialogClose = DialogPrimitive.Close;

function DialogPortal({
  children,
  ...props
}: DialogPrimitive.DialogPortalProps) {
  return <DialogPrimitive.Portal {...props}>{children}</DialogPrimitive.Portal>;
}

function DialogOverlay({
  className,
  ...props
}: React.ComponentPropsWithoutRef<typeof DialogPrimitive.Overlay>) {
  return (
    <DialogPrimitive.Overlay
      className={cn(
        "data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 fixed inset-0 z-overlay-scrim bg-overlay-scrim backdrop-blur-[2px] duration-fast ease-iris-out",
        className,
      )}
      {...props}
    />
  );
}

type DialogContentProps = React.ComponentPropsWithoutRef<
  typeof DialogPrimitive.Content
> & {
  size?: IrisOverlaySize;
  /** 默认 true；自管顶栏关闭钮时传 false */
  showClose?: boolean;
};

function DialogContent({
  className,
  children,
  size = "compact",
  showClose = true,
  ...props
}: DialogContentProps) {
  return (
    <DialogPortal>
      <DialogOverlay />
      <DialogPrimitive.Content
        aria-describedby={undefined}
        className={irisOverlayPanelClass(size, className)}
        {...props}
      >
        {children}
        {showClose ? (
          <DialogPrimitive.Close className="iris-focus-soft absolute right-3 top-3 inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground duration-fast ease-iris-out hover:bg-muted hover:text-foreground focus:outline-none">
            <X className="h-4 w-4" />
            <span className="sr-only">关闭</span>
          </DialogPrimitive.Close>
        ) : null}
      </DialogPrimitive.Content>
    </DialogPortal>
  );
}

function DialogHeader({
  className,
  ...props
}: React.ComponentPropsWithoutRef<"div">) {
  return (
    <div
      className={cn("flex flex-col space-y-1.5 px-4 pb-2 pt-4", className)}
      {...props}
    />
  );
}

function DialogTitle({
  className,
  ...props
}: React.ComponentPropsWithoutRef<typeof DialogPrimitive.Title>) {
  return (
    <DialogPrimitive.Title
      className={cn("text-sm font-medium leading-tight", className)}
      {...props}
    />
  );
}

function DialogFooter({
  className,
  ...props
}: React.ComponentPropsWithoutRef<"div">) {
  return (
    <div
      className={cn(
        "flex flex-col-reverse gap-2 px-4 pb-4 sm:flex-row sm:justify-end",
        className,
      )}
      {...props}
    />
  );
}

function DialogDescription({
  className,
  ...props
}: React.ComponentPropsWithoutRef<typeof DialogPrimitive.Description>) {
  return (
    <DialogPrimitive.Description
      className={cn("text-xs text-muted-foreground", className)}
      {...props}
    />
  );
}

export {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
  DialogPortal,
  DialogTitle,
  DialogTrigger,
};
