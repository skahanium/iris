import type { Editor } from "@tiptap/react";
import { TextSelection } from "@tiptap/pm/state";
import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";

import {
  activeOutlineIndex,
  outlineFromDoc,
  type OutlineEntry,
} from "@/lib/document-outline";
import { cn } from "@/lib/utils";

const LEVEL_STYLES: Record<number, { indent: string }> = {
  1: {
    indent: "0rem",
  },
  2: {
    indent: "0.55rem",
  },
  3: {
    indent: "1.1rem",
  },
};

interface EditorOutlineProps {
  editor: Editor | null;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  locked?: boolean;
  zen?: boolean;
}

const OUTLINE_REFRESH_DEBOUNCE_MS = 300;

function closestHeadingElement(node: Node | null): HTMLElement | null {
  const element =
    node instanceof HTMLElement ? node : (node?.parentElement ?? null);
  return (
    element?.closest<HTMLElement>("h1,h2,h3,.iris-section-heading") ?? null
  );
}

function headingElementForPos(editor: Editor, pos: number): HTMLElement | null {
  const doc = editor.view.dom;
  const nodeAtHeadingStart = editor.view.nodeDOM(Math.max(0, pos - 1));
  const directHeading = closestHeadingElement(nodeAtHeadingStart);
  if (directHeading && doc.contains(directHeading)) return directHeading;

  const domAtPos = editor.view.domAtPos(pos).node;
  const fallbackHeading = closestHeadingElement(domAtPos);
  return fallbackHeading && doc.contains(fallbackHeading)
    ? fallbackHeading
    : null;
}

function scrollHeadingToViewportTop(editor: Editor, pos: number): void {
  headingElementForPos(editor, pos)?.scrollIntoView({
    block: "start",
    inline: "nearest",
  });
}

export const EditorOutline = memo(function EditorOutline({
  editor,
  locked = false,
  zen = false,
}: EditorOutlineProps) {
  const [entries, setEntries] = useState<OutlineEntry[]>([]);
  const [activeIndex, setActiveIndex] = useState(-1);
  const [hoverIndex, setHoverIndex] = useState<number | null>(null);
  const [focusIndex, setFocusIndex] = useState<number | null>(null);
  const entriesRef = useRef<OutlineEntry[]>([]);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);
  const itemRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const barRef = useRef<HTMLDivElement | null>(null);
  const relativeLevelByHeadingLevel = useMemo(() => {
    const levels = Array.from(
      new Set(entries.map((entry) => entry.level)),
    ).sort((a, b) => a - b);
    return new Map(
      levels.map((level, index) => [
        level,
        Math.min(index + 1, 3) as 1 | 2 | 3,
      ]),
    );
  }, [entries]);

  useEffect(() => {
    if (!editor) return;

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
  }, [editor]);

  useEffect(() => {
    if (activeIndex < 0) return;
    itemRefs.current[activeIndex]?.scrollIntoView({ block: "nearest" });
  }, [activeIndex]);

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

    const listRect = listEl.getBoundingClientRect();
    const targetRect = target.getBoundingClientRect();
    const top = targetRect.top - listRect.top + listEl.scrollTop;
    const height = targetRect.height;

    bar.style.opacity = "1";
    bar.style.transform = `translateY(${top}px)`;
    bar.style.height = `${height}px`;
  }, [activeIndex, entries.length]);

  const jumpTo = useCallback(
    (pos: number) => {
      if (!editor) return;
      const { doc } = editor.state;
      const resolvedPos = Math.max(0, Math.min(pos, doc.content.size));
      if (locked) {
        const selection = TextSelection.create(doc, resolvedPos);
        editor.view.dispatch(editor.state.tr.setSelection(selection));
        scrollHeadingToViewportTop(editor, resolvedPos);
        return;
      }
      editor.chain().focus().setTextSelection(resolvedPos).run();
      scrollHeadingToViewportTop(editor, resolvedPos);
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
      itemRefs.current[next]?.scrollIntoView({ block: "nearest" });
    },
    [activeIndex, entries.length, focusIndex, hoverIndex],
  );

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
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
    [activeIndex, entries, focusIndex, jumpTo, moveFocus],
  );

  const renderItem = (entry: OutlineEntry, index: number) => {
    const active = index === activeIndex;
    const focused = index === focusIndex;
    const hovered = index === hoverIndex;
    const activeDistance = activeIndex >= 0 ? Math.abs(index - activeIndex) : 0;
    const candidateIndex = hoverIndex ?? focusIndex;
    const candidate = index === candidateIndex;
    const candidateDistance =
      candidateIndex != null ? Math.abs(index - candidateIndex) : 0;
    const relativeLevel = relativeLevelByHeadingLevel.get(entry.level) ?? 1;
    const lvl = LEVEL_STYLES[relativeLevel]!;
    const itemStyle: CSSProperties = {
      "--outline-bar-indent": lvl.indent,
      paddingLeft: `calc(${lvl.indent} + var(--editor-outline-bar-offset))`,
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
          "outline-ghost-item flex w-full items-center text-left text-[hsl(var(--outline-rail-active))]",
          `outline-ghost-item--level-${relativeLevel}`,
          active &&
            "outline-ghost-item--active text-[hsl(var(--outline-rail-active))]",
          !active && activeDistance === 1 && "outline-ghost-item--near-1",
          !active && activeDistance === 2 && "outline-ghost-item--near-2",
          candidate && "outline-ghost-item--candidate",
          !candidate &&
            candidateDistance === 1 &&
            "outline-ghost-item--candidate-near-1",
          !candidate &&
            candidateDistance === 2 &&
            "outline-ghost-item--candidate-near-2",
          focused && "outline-ghost-item--focused",
          hovered && "outline-ghost-item--hovered",
        )}
        style={itemStyle}
        aria-current={active ? "location" : undefined}
        aria-label={entry.text}
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          jumpTo(entry.pos);
        }}
        onClick={() => jumpTo(entry.pos)}
        onFocus={() => {
          setFocusIndex(index);
          setHoverIndex(null);
        }}
        onPointerEnter={() => {
          setHoverIndex(index);
          setFocusIndex(null);
        }}
      >
        <span className="outline-ghost-item-line" aria-hidden />
      </button>
    );
  };

  const previewIndex = hoverIndex ?? focusIndex;
  const previewEntry =
    previewIndex != null && previewIndex >= 0 ? entries[previewIndex] : null;

  if (zen) return null;

  return (
    <nav
      data-testid="outline-rail"
      className="outline-ghost outline-ghost--active pointer-events-auto absolute z-editor-chrome flex w-[var(--editor-outline-rail-width)] min-w-[var(--editor-outline-rail-width)] flex-col"
      style={{ left: "var(--editor-outline-inset)" }}
      aria-label="文档目录"
      tabIndex={0}
      onKeyDown={handleKeyDown}
      onPointerLeave={() => {
        setHoverIndex(null);
      }}
    >
      <div
        ref={listRef}
        className="outline-ghost-list outline-ghost-bar-track flex flex-col"
        role="list"
      >
        <div ref={barRef} className="outline-ghost-bar" aria-hidden />
        <div className="outline-ghost-items">
          {entries.length === 0 ? (
            <span className="outline-ghost-empty">暂无章节</span>
          ) : (
            entries.map((entry, index) => renderItem(entry, index))
          )}
        </div>
      </div>
      {previewEntry ? (
        <aside
          data-testid="outline-ghost-popover"
          className="outline-ghost-popover"
          aria-live="polite"
        >
          <div className="outline-ghost-popover-kicker">
            H{previewEntry.level}
          </div>
          <div className="outline-ghost-popover-title">{previewEntry.text}</div>
        </aside>
      ) : null}
    </nav>
  );
});
