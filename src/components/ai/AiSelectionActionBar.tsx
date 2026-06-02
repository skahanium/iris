import { ArrowDownToLine, Copy, Download, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface AiSelectionActionBarProps {
  count: number;
  onInsert?: () => void;
  onCopy?: () => void;
  onExport?: () => void;
  onClear: () => void;
  className?: string;
}

/**
 * AI 对话区选中消息后的浮动操作栏。
 *
 * 显示选中数量和操作按钮：插入编辑器、复制、导出、取消选中。
 */
export function AiSelectionActionBar({
  count,
  onInsert,
  onCopy,
  onExport,
  onClear,
  className,
}: AiSelectionActionBarProps) {
  if (count === 0) return null;

  return (
    <div
      className={cn(
        "flex items-center gap-2 rounded-lg border border-border/60 bg-panel/95 px-3 py-2 text-xs shadow-lg backdrop-blur-sm",
        className,
      )}
    >
      <span className="text-muted-foreground">已选 {count} 条</span>
      <div className="mx-1 h-4 w-px bg-border/60" />
      {onInsert ? (
        <Button
          variant="ghost"
          size="sm"
          className="h-7 gap-1 px-2 text-xs"
          onClick={onInsert}
        >
          <ArrowDownToLine className="h-3.5 w-3.5" />
          插入编辑器
        </Button>
      ) : null}
      {onCopy ? (
        <Button
          variant="ghost"
          size="sm"
          className="h-7 gap-1 px-2 text-xs"
          onClick={onCopy}
        >
          <Copy className="h-3.5 w-3.5" />
          复制
        </Button>
      ) : null}
      {onExport ? (
        <Button
          variant="ghost"
          size="sm"
          className="h-7 gap-1 px-2 text-xs"
          onClick={onExport}
        >
          <Download className="h-3.5 w-3.5" />
          导出
        </Button>
      ) : null}
      <Button
        variant="ghost"
        size="sm"
        className="h-7 w-7 p-0"
        onClick={onClear}
      >
        <X className="h-3.5 w-3.5" />
      </Button>
    </div>
  );
}
