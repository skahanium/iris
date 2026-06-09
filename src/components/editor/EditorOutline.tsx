import type { Editor } from "@tiptap/react";
import { ListTree, X } from "lucide-react";
import { memo, useEffect, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import {
  activeOutlineIndex,
  outlineFromDoc,
  type OutlineEntry,
} from "@/lib/document-outline";
import { cn } from "@/lib/utils";

interface EditorOutlineProps {
  editor: Editor | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  zen?: boolean;
}

export const EditorOutline = memo(function EditorOutline({
  editor,
  open,
  onOpenChange,
  zen = false,
}: EditorOutlineProps) {
  const [entries, setEntries] = useState<OutlineEntry[]>([]);
  const [activeIndex, setActiveIndex] = useState(-1);
  const entriesRef = useRef<OutlineEntry[]>([]);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!editor || !open) return;

    const updateOutline = () => {
      const next = outlineFromDoc(editor.state.doc);
      entriesRef.current = next;
      setEntries(next);
      setActiveIndex(activeOutlineIndex(next, editor.state.selection.head));
    };

    const updateActiveIndex = () => {
      setActiveIndex(
        activeOutlineIndex(entriesRef.current, editor.state.selection.head),
      );
    };

    const debouncedUpdate = () => {
      if (timerRef.current) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => {
        timerRef.current = null;
        updateOutline();
      }, 300);
    };

    // 初始立即更新
    updateOutline();

    editor.on("update", debouncedUpdate);
    editor.on("selectionUpdate", updateActiveIndex);
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
      editor.off("update", debouncedUpdate);
      editor.off("selectionUpdate", updateActiveIndex);
    };
  }, [editor, open]);

  const jumpTo = (pos: number) => {
    if (!editor) return;
    editor.chain().focus().setTextSelection(pos).scrollIntoView().run();
  };

  if (zen) return null;

  if (!open) {
    return (
      <button
        type="button"
        data-testid="outline-rail-handle"
        className="outline-rail-handle pointer-events-auto absolute z-editor-chrome"
        style={{ left: "var(--editor-outline-inset)" }}
        aria-label="显示目录"
        onClick={() => onOpenChange(true)}
      >
        <ListTree className="h-3.5 w-3.5" />
        <span className="sr-only">目录</span>
      </button>
    );
  }

  return (
    <nav
      data-testid="outline-rail"
      className="outline-rail pointer-events-none absolute z-editor-chrome flex max-h-[min(70dvh,28rem)] w-[var(--editor-outline-width)] flex-col"
      style={{ left: "var(--editor-outline-inset)" }}
      aria-label="文档目录"
    >
      <div className="pointer-events-auto flex min-h-0 flex-col overflow-hidden">
        <div className="flex shrink-0 items-center justify-between gap-1.5 border-b border-border/60 px-2 py-1.5">
          <span className="font-sans text-xs font-medium text-foreground">
            目录
          </span>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="h-7 w-7 shrink-0 p-0"
            aria-label="隐藏目录"
            onClick={() => onOpenChange(false)}
          >
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>
        <ol className="min-h-0 flex-1 overflow-y-auto px-1.5 py-1.5 font-sans text-xs">
          {entries.length === 0 ? (
            <li className="px-1.5 py-2 text-muted-foreground">暂无章节标题</li>
          ) : (
            entries.map((entry, index) => (
              <li key={`${entry.pos}-${entry.text}`}>
                <button
                  type="button"
                  className={cn(
                    "outline-rail-item",
                    `outline-rail-item--level-${entry.level}`,
                    index === activeIndex && "outline-rail-item--active",
                  )}
                  onClick={() => jumpTo(entry.pos)}
                >
                  <span className="line-clamp-2">{entry.text}</span>
                </button>
              </li>
            ))
          )}
        </ol>
      </div>
    </nav>
  );
});
