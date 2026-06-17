import type { Editor } from "@tiptap/react";
import { TextSelection } from "@tiptap/pm/state";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Link2, ListTree } from "lucide-react";
import {
  memo,
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
} from "react";

import {
  activeOutlineIndex,
  outlineFromDoc,
  type OutlineEntry,
} from "@/lib/document-outline";
import { fileLinkSummary } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import type { FileLinkPreview, FileLinkSummary } from "@/types/ipc";

const LEVEL_STYLES: Record<number, { fontSize: string; indent: string }> = {
  1: {
    fontSize: "0.95rem",
    indent: "0rem",
  },
  2: {
    fontSize: "0.82rem",
    indent: "1.35rem",
  },
  3: {
    fontSize: "0.72rem",
    indent: "2.5rem",
  },
};

interface EditorOutlineProps {
  editor: Editor | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  notePath?: string | null;
  onOpenNote?: (path: string) => void;
  locked?: boolean;
  zen?: boolean;
}

const OUTLINE_REFRESH_DEBOUNCE_MS = 300;
const VIRTUAL_OUTLINE_THRESHOLD = 50;
const OUTLINE_ROW_HEIGHT = 56;

interface OutlineLinkSummaryProps {
  summary: FileLinkSummary | null;
  unavailable: boolean;
  onOpenNote?: (path: string) => void;
}

function OutlineLinkItems({
  items,
  onOpenNote,
}: {
  items: FileLinkPreview[];
  onOpenNote?: (path: string) => void;
}) {
  if (items.length === 0) {
    return <span className="outline-link-summary-empty">暂无链接</span>;
  }

  return (
    <div className="outline-link-summary-items">
      {items.slice(0, 3).map((item) => (
        <button
          key={item.path}
          type="button"
          data-testid="outline-link-summary-item"
          className="outline-link-summary-item"
          title={item.context ?? item.path}
          onClick={() => onOpenNote?.(item.path)}
          onKeyDown={(event) => {
            event.stopPropagation();
          }}
        >
          {item.title}
        </button>
      ))}
    </div>
  );
}

function OutlineLinkSummary({
  summary,
  unavailable,
  onOpenNote,
}: OutlineLinkSummaryProps) {
  if (unavailable) {
    return (
      <aside
        data-testid="outline-link-summary"
        className="outline-link-summary"
        aria-label="双链摘要"
      >
        <div className="outline-link-summary-heading">
          <Link2 className="h-3 w-3" aria-hidden />
          <span>双链</span>
        </div>
        <span className="outline-link-summary-empty">暂不可用</span>
      </aside>
    );
  }

  if (!summary) return null;

  const hasLinks = summary.inboundCount > 0 || summary.outboundCount > 0;

  return (
    <aside
      data-testid="outline-link-summary"
      className="outline-link-summary"
      aria-label="双链摘要"
    >
      <div className="outline-link-summary-heading">
        <Link2 className="h-3 w-3" aria-hidden />
        <span>双链</span>
      </div>
      <div className="outline-link-summary-counts">
        <span>{summary.inboundCount} 入链</span>
        <span>{summary.outboundCount} 出链</span>
      </div>
      {hasLinks ? (
        <div className="outline-link-summary-groups">
          <div className="outline-link-summary-group">
            <span className="outline-link-summary-label">指向此文档</span>
            <OutlineLinkItems items={summary.inbound} onOpenNote={onOpenNote} />
          </div>
          <div className="outline-link-summary-group">
            <span className="outline-link-summary-label">本文指向</span>
            <OutlineLinkItems
              items={summary.outbound}
              onOpenNote={onOpenNote}
            />
          </div>
        </div>
      ) : (
        <span className="outline-link-summary-empty">还没有双链</span>
      )}
    </aside>
  );
}

export const EditorOutline = memo(function EditorOutline({
  editor,
  open,
  onOpenChange,
  notePath = null,
  onOpenNote,
  locked = false,
  zen = false,
}: EditorOutlineProps) {
  const [entries, setEntries] = useState<OutlineEntry[]>([]);
  const [activeIndex, setActiveIndex] = useState(-1);
  const [hoverIndex, setHoverIndex] = useState<number | null>(null);
  const [focusIndex, setFocusIndex] = useState<number | null>(null);
  const [linkSummary, setLinkSummary] = useState<FileLinkSummary | null>(null);
  const [linkSummaryUnavailable, setLinkSummaryUnavailable] = useState(false);
  const entriesRef = useRef<OutlineEntry[]>([]);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);
  const itemRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const barRef = useRef<HTMLDivElement | null>(null);
  const shouldVirtualize = entries.length >= VIRTUAL_OUTLINE_THRESHOLD;

  const rowVirtualizer = useVirtualizer({
    count: entries.length,
    getScrollElement: () => listRef.current,
    estimateSize: () => OUTLINE_ROW_HEIGHT,
    overscan: 8,
    enabled: shouldVirtualize,
  });

  useEffect(() => {
    if (!editor || !open) return;

    const updateOutline = () => {
      const next = outlineFromDoc(editor.state.doc);
      entriesRef.current = next;
      itemRefs.current = itemRefs.current.slice(0, next.length);
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
      }, OUTLINE_REFRESH_DEBOUNCE_MS);
    };

    updateOutline();

    editor.on("update", debouncedUpdate);
    editor.on("selectionUpdate", updateActiveIndex);
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
      editor.off("update", debouncedUpdate);
      editor.off("selectionUpdate", updateActiveIndex);
    };
  }, [editor, open]);

  useEffect(() => {
    if (!open) {
      setHoverIndex(null);
      setFocusIndex(null);
      setLinkSummary(null);
      setLinkSummaryUnavailable(false);
    }
  }, [open]);

  useEffect(() => {
    if (!open || !notePath) return;

    let cancelled = false;
    setLinkSummaryUnavailable(false);

    void fileLinkSummary(notePath)
      .then((summary) => {
        if (cancelled) return;
        setLinkSummary(summary);
      })
      .catch(() => {
        if (cancelled) return;
        setLinkSummary(null);
        setLinkSummaryUnavailable(true);
      });

    return () => {
      cancelled = true;
    };
  }, [notePath, open]);

  useEffect(() => {
    if (!open || activeIndex < 0) return;
    if (shouldVirtualize) {
      rowVirtualizer.scrollToIndex(activeIndex, { align: "auto" });
      return;
    }
    itemRefs.current[activeIndex]?.scrollIntoView({ block: "nearest" });
  }, [activeIndex, open, rowVirtualizer, shouldVirtualize]);

  // Sliding indicator bar position
  useEffect(() => {
    const bar = barRef.current;
    if (!bar) return;

    if (activeIndex < 0 || entries.length === 0) {
      bar.style.opacity = "0";
      return;
    }

    const target = itemRefs.current[activeIndex];
    if (!target) {
      bar.style.opacity = "0";
      return;
    }

    const listEl = listRef.current;
    if (!listEl) return;

    // For virtualized lists, use the virtualizer's offset
    if (shouldVirtualize) {
      const virtualItem = rowVirtualizer
        .getVirtualItems()
        .find((vi) => vi.index === activeIndex);
      if (virtualItem) {
        bar.style.opacity = "1";
        bar.style.transform = `translateY(${virtualItem.start}px)`;
        bar.style.height = `${virtualItem.size}px`;
      } else {
        bar.style.opacity = "0";
      }
      return;
    }

    const listRect = listEl.getBoundingClientRect();
    const targetRect = target.getBoundingClientRect();
    const top = targetRect.top - listRect.top + listEl.scrollTop;
    const height = targetRect.height;

    bar.style.opacity = "1";
    bar.style.transform = `translateY(${top}px)`;
    bar.style.height = `${height}px`;
  }, [activeIndex, entries.length, rowVirtualizer, shouldVirtualize]);

  const jumpTo = useCallback(
    (pos: number) => {
      if (!editor) return;
      if (locked) {
        const { doc } = editor.state;
        const resolvedPos = Math.max(0, Math.min(pos, doc.content.size));
        const selection = TextSelection.create(doc, resolvedPos);
        editor.view.dispatch(
          editor.state.tr.setSelection(selection).scrollIntoView(),
        );
        const targetNode = editor.view.nodeDOM(resolvedPos);
        const targetElement =
          targetNode instanceof Element
            ? targetNode
            : targetNode?.parentElement;
        targetElement?.scrollIntoView({ block: "start" });
        return;
      }
      editor.chain().focus().setTextSelection(pos).scrollIntoView().run();
    },
    [editor, locked],
  );

  const moveFocus = useCallback(
    (direction: -1 | 1) => {
      if (entries.length === 0) return;
      const current =
        focusIndex ?? hoverIndex ?? (activeIndex >= 0 ? activeIndex : 0);
      const next = Math.max(
        0,
        Math.min(entries.length - 1, current + direction),
      );
      setFocusIndex(next);
      setHoverIndex(null);
      if (shouldVirtualize) {
        rowVirtualizer.scrollToIndex(next, { align: "auto" });
      } else {
        itemRefs.current[next]?.scrollIntoView({ block: "nearest" });
      }
    },
    [
      activeIndex,
      entries.length,
      focusIndex,
      hoverIndex,
      rowVirtualizer,
      shouldVirtualize,
    ],
  );

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onOpenChange(false);
        return;
      }

      if (entries.length === 0) return;

      if (event.key === "ArrowDown") {
        event.preventDefault();
        moveFocus(1);
        return;
      }

      if (event.key === "ArrowUp") {
        event.preventDefault();
        moveFocus(-1);
        return;
      }

      if (event.key === "Enter") {
        event.preventDefault();
        const index = focusIndex ?? (activeIndex >= 0 ? activeIndex : -1);
        const entry = index >= 0 ? entries[index] : undefined;
        if (entry) jumpTo(entry.pos);
      }
    },
    [activeIndex, entries, focusIndex, jumpTo, moveFocus, onOpenChange],
  );

  const renderItem = (entry: OutlineEntry, index: number) => {
    const active = index === activeIndex;
    const focused = index === focusIndex;
    const hovered = index === hoverIndex;
    const activeDistance = activeIndex >= 0 ? Math.abs(index - activeIndex) : 0;
    const lvl = LEVEL_STYLES[entry.level]!;
    const itemStyle: CSSProperties = {
      "--outline-level-size": lvl.fontSize,
      "--outline-text-indent": lvl.indent,
      paddingLeft: `calc(${lvl.indent} + 0.5rem)`,
    } as CSSProperties;
    return (
      <button
        key={`${entry.pos}-${entry.text}`}
        ref={(node) => {
          itemRefs.current[index] = node;
        }}
        type="button"
        data-testid="outline-ghost-item"
        className={cn(
          "outline-ghost-item flex w-full items-center text-left",
          `outline-ghost-item--level-${entry.level}`,
          active && "outline-ghost-item--active",
          !active && activeDistance === 1 && "outline-ghost-item--near-1",
          !active && activeDistance === 2 && "outline-ghost-item--near-2",
          focused && "outline-ghost-item--focused",
          hovered && "outline-ghost-item--hovered",
        )}
        style={itemStyle}
        aria-current={active ? "location" : undefined}
        aria-label={entry.text}
        onClick={() => jumpTo(entry.pos)}
        onFocus={() => {
          setFocusIndex(index);
          setHoverIndex(null);
        }}
        onPointerEnter={() => {
          setHoverIndex(index);
          setFocusIndex(null);
        }}
        onPointerLeave={() => {
          setHoverIndex(null);
        }}
      >
        <span
          className="outline-ghost-text block min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-left"
          title={entry.text}
        >
          {entry.text}
        </span>
      </button>
    );
  };

  if (zen) return null;

  if (!open) {
    return (
      <button
        type="button"
        data-testid="outline-rail-handle"
        className="outline-ghost outline-ghost-handle pointer-events-auto absolute z-editor-chrome"
        style={{ left: "var(--editor-outline-inset)" }}
        aria-label="显示目录"
        onClick={() => onOpenChange(true)}
      >
        <ListTree className="h-3.5 w-3.5" />
        <span className="sr-only">目录</span>
      </button>
    );
  }

  const virtualItems = shouldVirtualize ? rowVirtualizer.getVirtualItems() : [];

  return (
    <nav
      data-testid="outline-rail"
      className="outline-ghost outline-ghost--active pointer-events-auto absolute z-editor-chrome flex w-[var(--editor-outline-rail-width)] min-w-[var(--editor-outline-rail-width)] flex-col"
      style={{ left: "var(--editor-outline-inset)" }}
      aria-label="文档目录"
      tabIndex={0}
      onKeyDown={handleKeyDown}
    >
      <button
        type="button"
        className="outline-ghost-handle outline-ghost-handle--embedded"
        aria-label="隐藏目录"
        onClick={() => onOpenChange(false)}
      >
        <ListTree className="h-3.5 w-3.5" />
      </button>
      <div
        ref={listRef}
        className="outline-ghost-list flex flex-col"
        role="list"
      >
        <div ref={barRef} className="outline-ghost-bar" aria-hidden />
        {entries.length === 0 ? (
          <span className="outline-ghost-empty">暂无章节</span>
        ) : shouldVirtualize ? (
          <div
            className="outline-ghost-virtual"
            style={{ height: `${rowVirtualizer.getTotalSize()}px` }}
          >
            {virtualItems.map((virtualItem) => {
              const entry = entries[virtualItem.index];
              if (!entry) return null;
              return (
                <div
                  key={virtualItem.key}
                  className="outline-ghost-virtual-row"
                  style={
                    {
                      height: `${virtualItem.size}px`,
                      transform: `translateY(${virtualItem.start}px)`,
                    } as CSSProperties
                  }
                >
                  {renderItem(entry, virtualItem.index)}
                </div>
              );
            })}
          </div>
        ) : (
          entries.map((entry, index) => renderItem(entry, index))
        )}
      </div>
      <OutlineLinkSummary
        summary={linkSummary}
        unavailable={linkSummaryUnavailable}
        onOpenNote={onOpenNote}
      />
    </nav>
  );
});
