import { Search, X } from "lucide-react";
import type { KeyboardEvent, ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface OverlaySearchHeaderProps {
  placeholder: string;
  value: string;
  onChange: (value: string) => void;
  onKeyDown?: (event: KeyboardEvent<HTMLInputElement>) => void;
  onClose?: () => void;
  /** 隐藏右侧关闭钮时仅保留 Esc/scrim */
  showClose?: boolean;
  autoFocus?: boolean;
  inputAriaLabel?: string;
  className?: string;
}

/** 命令浮层 / Quick Open 统一顶栏 */
export function OverlaySearchHeader({
  placeholder,
  value,
  onChange,
  onKeyDown,
  onClose,
  showClose = true,
  autoFocus = true,
  inputAriaLabel = "搜索",
  className,
}: OverlaySearchHeaderProps) {
  return (
    <div
      className={cn(
        "task-overlay-header flex h-12 shrink-0 items-center gap-2 border-b border-border/60 bg-surface-elevated px-3",
        className,
      )}
    >
      <Search className="h-4 w-4 shrink-0 text-muted-foreground" aria-hidden />
      <input
        type="search"
        className="min-w-0 flex-1 bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground"
        placeholder={placeholder}
        value={value}
        autoFocus={autoFocus}
        aria-label={inputAriaLabel}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={onKeyDown}
      />
      {showClose && onClose ? (
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-8 w-8 shrink-0"
          aria-label="关闭"
          onClick={onClose}
        >
          <X className="h-4 w-4" />
        </Button>
      ) : null}
    </div>
  );
}

interface OverlayChromeProps {
  header: ReactNode;
  footer?: ReactNode;
  children: ReactNode;
  className?: string;
}

/** 浮层内容区壳：顶栏 + 主体 + 可选底栏 */
export function OverlayChrome({
  header,
  footer,
  children,
  className,
}: OverlayChromeProps) {
  return (
    <div
      className={cn("flex min-h-0 flex-1 flex-col overflow-hidden", className)}
    >
      {header}
      <div className="task-overlay-body min-h-0 flex-1 overflow-hidden">
        {children}
      </div>
      {footer ? <div className="task-overlay-footer">{footer}</div> : null}
    </div>
  );
}
