import { Send, Square } from "lucide-react";
import type { KeyboardEvent, ReactNode, RefObject } from "react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface AiComposerProps {
  value: string;
  onChange: (value: string) => void;
  onSubmit: () => void;
  onStop?: () => void;
  streaming?: boolean;
  disabled?: boolean;
  placeholder?: string;
  className?: string;
  textareaRef?: RefObject<HTMLTextAreaElement | null>;
  onComposerKeyDown?: (e: KeyboardEvent<HTMLTextAreaElement>) => void;
  onSelect?: () => void;
  /** 渲染在输入框圆角容器内、文本区正上方（如 @ 补全）。 */
  mentionPopover?: ReactNode;
  /** @deprecated 工具/检索状态已移至底栏，保留以兼容旧调用 */
  statusHint?: string | null;
}

/** AI 侧栏多行输入区 */
export function AiComposer({
  value,
  onChange,
  onSubmit,
  onStop,
  streaming = false,
  disabled = false,
  placeholder = "提问…",
  className,
  textareaRef,
  onComposerKeyDown,
  onSelect,
  mentionPopover,
}: AiComposerProps) {
  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    onComposerKeyDown?.(e);
    if (e.defaultPrevented) return;
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      if (!streaming && value.trim()) onSubmit();
    }
  };

  return (
    <div
      className={cn(
        "shrink-0 border-t border-border/60 bg-ai-composer p-3",
        className,
      )}
    >
      <div className="relative flex items-end gap-2 rounded-lg border border-border/80 bg-surface-inset/50 p-2 shadow-sm focus-within:ring-2 focus-within:ring-primary/25">
        {mentionPopover ? (
          <div className="absolute bottom-full left-0 right-0 z-20 mb-1.5">
            {mentionPopover}
          </div>
        ) : null}
        <textarea
          ref={textareaRef}
          rows={2}
          value={value}
          disabled={disabled && !streaming}
          placeholder={placeholder}
          aria-label="AI 输入"
          className="max-h-32 min-h-[2.5rem] min-w-0 flex-1 resize-none bg-transparent text-[15px] leading-[1.52] text-foreground outline-none placeholder:text-muted-foreground disabled:opacity-50"
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={handleKeyDown}
          onSelect={onSelect}
          onClick={onSelect}
        />
        {streaming && onStop ? (
          <Button
            type="button"
            size="icon"
            variant="secondary"
            className="h-9 w-9 shrink-0"
            aria-label="停止生成"
            onClick={onStop}
          >
            <Square className="h-3.5 w-3.5" />
          </Button>
        ) : (
          <Button
            type="button"
            size="icon"
            className="h-9 w-9 shrink-0"
            disabled={disabled || !value.trim()}
            aria-label="发送"
            onClick={onSubmit}
          >
            <Send className="h-4 w-4" />
          </Button>
        )}
      </div>
      <p className="mt-1.5 text-[10px] text-muted-foreground">
        Enter 发送 · Shift+Enter 换行
      </p>
    </div>
  );
}
