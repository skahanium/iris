import { AlertTriangle } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  message: string;
  /** 可选的详细描述，显示在 message 下方 */
  description?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  variant?: "default" | "destructive";
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({
  open,
  title,
  message,
  description,
  confirmLabel = "确认",
  cancelLabel = "取消",
  variant = "default",
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const isDestructive = variant === "destructive";

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onCancel()}>
      <DialogContent className="max-w-[26rem] gap-0 overflow-hidden p-0">
        {/* Header */}
        <DialogHeader className="gap-2 px-5 pb-0 pt-5">
          <div className="flex items-start gap-3">
            {isDestructive && (
              <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-destructive/10">
                <AlertTriangle className="h-4 w-4 text-destructive" />
              </div>
            )}
            <div className="min-w-0 flex-1">
              <DialogTitle
                className={cn("text-base", isDestructive && "text-foreground")}
              >
                {title}
              </DialogTitle>
              <DialogDescription className="mt-1.5 text-sm leading-relaxed text-muted-foreground">
                {message}
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>

        {/* Optional detail */}
        {description && (
          <div className="px-5 pb-0 pt-3">
            <p className="rounded-md bg-muted/60 px-3 py-2 text-xs leading-relaxed text-muted-foreground">
              {description}
            </p>
          </div>
        )}

        {/* Footer */}
        <DialogFooter className="gap-2 px-5 pb-5 pt-4 sm:justify-end">
          <Button
            type="button"
            variant="ghost"
            className="h-8 px-3 text-xs"
            onClick={onCancel}
          >
            {cancelLabel}
          </Button>
          <Button
            type="button"
            variant={isDestructive ? "destructive" : "default"}
            className="h-8 px-3 text-xs"
            onClick={onConfirm}
          >
            {confirmLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
