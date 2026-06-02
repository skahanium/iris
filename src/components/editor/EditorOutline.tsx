import type { Editor } from "@tiptap/react";
import { ListTree, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";

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

export const EDITOR_OUTLINE_REFRESH_MS = 160;

export function EditorOutline({
  editor,
  open,
  onOpenChange,
  zen = false,
}: EditorOutlineProps) {
  const [entries, setEntries] = useState<OutlineEntry[]>([]);
  const [activeIndex, setActiveIndex] = useState(-1);
  const entriesRef = useRef<OutlineEntry[]>([]);
  const refreshTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!editor || !open) return;

    const clearRefreshTimer = () => {
      if (refreshTimerRef.current !== null) {
        clearTimeout(refreshTimerRef.current);
        refreshTimerRef.current = null;
      }
    };

    const rebuildOutline = () => {
      clearRefreshTimer();
      const next = outlineFromDoc(editor.state.doc);
      entriesRef.current = next;
      setEntries(next);
      setActiveIndex(activeOutlineIndex(next, editor.state.selection.head));
    };

    const scheduleOutlineRefresh = () => {
      clearRefreshTimer();
      refreshTimerRef.current = setTimeout(
        rebuildOutline,
        EDITOR_OUTLINE_REFRESH_MS,
      );
    };

    const updateActiveOutline = () => {
      setActiveIndex(
        activeOutlineIndex(entriesRef.current, editor.state.selection.head),
      );
    };

    rebuildOutline();
    editor.on("update", scheduleOutlineRefresh);
    editor.on("selectionUpdate", updateActiveOutline);
    return () => {
      clearRefreshTimer();
      editor.off("update", scheduleOutlineRefresh);
      editor.off("selectionUpdate", updateActiveOutline);
    };
  }, [editor, open]);

  const jumpTo = (pos: number) => {
    if (!editor) return;
    editor.chain().focus().setTextSelection(pos).scrollIntoView().run();
  };

  if (zen) return null;

  if (!open) {
    return (
      <Button
        type="button"
        size="sm"
        variant="outline"
        className="pointer-events-auto absolute top-4 z-editor-chrome h-8 gap-1.5 rounded-md border border-border bg-panel px-2.5 text-xs shadow-floating"
        style={{ left: "var(--editor-outline-inset)" }}
        aria-label="显示目录"
        onClick={() => onOpenChange(true)}
      >
        <ListTree className="h-3.5 w-3.5" />
        目录
      </Button>
    );
  }

  return (
    <nav
      className="pointer-events-none absolute top-4 z-editor-chrome flex max-h-[min(70dvh,28rem)] w-[min(var(--editor-outline-width),32vw)] max-w-[10rem] flex-col"
      style={{ left: "var(--editor-outline-inset)" }}
      aria-label="文档目录"
    >
      <div className="pointer-events-auto flex min-h-0 flex-col overflow-hidden rounded-lg border border-border bg-panel/95 shadow-floating backdrop-blur-sm">
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
                    "w-full rounded-md px-1.5 py-1 text-left leading-snug transition-colors duration-150",
                    "hover:bg-muted/80",
                    entry.level === 2 && "pl-3",
                    entry.level === 3 && "pl-4",
                    index === activeIndex &&
                      "bg-primary/12 font-medium text-primary",
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
}
