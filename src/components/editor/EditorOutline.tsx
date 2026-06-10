import type { Editor } from "@tiptap/react";
import { ListTree } from "lucide-react";
import {
  memo,
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
  type WheelEvent as ReactWheelEvent,
} from "react";

import {
  activeOutlineIndex,
  outlineFromDoc,
  type OutlineEntry,
} from "@/lib/document-outline";
import {
  clampPointerY,
  getTickTop,
  nearestIndexFromPointer,
  stepScrubIndex,
  wheelScrubIndex,
} from "@/lib/outline-luminous";
import { cn } from "@/lib/utils";

import { OutlineLuminousCaption } from "./OutlineLuminousCaption";

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
  const [hoverIndex, setHoverIndex] = useState<number | null>(null);
  const [focusIndex, setFocusIndex] = useState<number | null>(null);
  const entriesRef = useRef<OutlineEntry[]>([]);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const trackRef = useRef<HTMLDivElement | null>(null);

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
    }
  }, [open]);

  const jumpTo = useCallback(
    (pos: number) => {
      if (!editor) return;
      editor.chain().focus().setTextSelection(pos).scrollIntoView().run();
    },
    [editor],
  );

  const scrubFromClientY = useCallback(
    (clientY: number) => {
      const track = trackRef.current;
      if (!track || entries.length === 0) return;
      const rect = track.getBoundingClientRect();
      if (rect.height <= 0) return;
      const pointerY = clampPointerY(clientY - rect.top, rect.height);
      const index = nearestIndexFromPointer(
        pointerY,
        rect.height,
        entries.length,
      );
      setHoverIndex(index);
      setFocusIndex(null);
    },
    [entries.length],
  );

  const handleTrackPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (entries.length === 0) return;
    scrubFromClientY(event.clientY);
  };

  const handleTrackPointerLeave = () => {
    setHoverIndex(null);
  };

  const handleWheel = (event: ReactWheelEvent<HTMLDivElement>) => {
    if (entries.length === 0) return;
    event.preventDefault();
    const current =
      focusIndex ?? hoverIndex ?? (activeIndex >= 0 ? activeIndex : 0);
    const next = wheelScrubIndex(event.deltaY, current, entries.length);
    setFocusIndex(next);
    setHoverIndex(next);
  };

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      if (entries.length === 0) return;

      if (event.key === "Escape") {
        event.preventDefault();
        onOpenChange(false);
        return;
      }

      if (event.key === "ArrowDown") {
        event.preventDefault();
        const current =
          focusIndex ?? hoverIndex ?? (activeIndex >= 0 ? activeIndex : 0);
        const next = stepScrubIndex(current, entries.length, 1);
        setFocusIndex(next);
        setHoverIndex(next);
        return;
      }

      if (event.key === "ArrowUp") {
        event.preventDefault();
        const current =
          focusIndex ?? hoverIndex ?? (activeIndex >= 0 ? activeIndex : 0);
        const next = stepScrubIndex(current, entries.length, -1);
        setFocusIndex(next);
        setHoverIndex(next);
        return;
      }

      if (event.key === "Enter") {
        event.preventDefault();
        const index =
          focusIndex ?? hoverIndex ?? (activeIndex >= 0 ? activeIndex : -1);
        const entry = index >= 0 ? entries[index] : undefined;
        if (entry) jumpTo(entry.pos);
      }
    },
    [entries, focusIndex, hoverIndex, activeIndex, jumpTo, onOpenChange],
  );

  const showScrubCaption = hoverIndex !== null || focusIndex !== null;
  const captionIndex = showScrubCaption
    ? (hoverIndex ?? focusIndex)
    : activeIndex >= 0
      ? activeIndex
      : null;
  const captionVariant =
    hoverIndex !== null
      ? "hover"
      : focusIndex !== null
        ? "focus"
        : "active";
  const total = entries.length;

  if (zen) return null;

  if (!open) {
    return (
      <button
        type="button"
        data-testid="outline-rail-handle"
        className="outline-luminous outline-luminous-handle pointer-events-auto absolute z-editor-chrome"
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
      className="outline-luminous outline-luminous--active pointer-events-auto absolute z-editor-chrome"
      style={{ left: "var(--editor-outline-inset)" }}
      aria-label="文档目录光轨"
      tabIndex={0}
      onKeyDown={handleKeyDown}
    >
      <button
        type="button"
        className="outline-luminous-handle outline-luminous-handle--embedded"
        aria-label="隐藏目录"
        onClick={() => onOpenChange(false)}
      >
        <ListTree className="h-3.5 w-3.5" />
      </button>
      <div
        ref={trackRef}
        className="outline-luminous-track"
        onPointerMove={handleTrackPointerMove}
        onPointerLeave={handleTrackPointerLeave}
        onWheel={handleWheel}
      >
        {activeIndex >= 0 && total > 0 && (
          <div
            className="outline-luminous-active-beacon"
            aria-hidden
            style={
              {
                "--outline-tick-top": `${getTickTop(activeIndex, total)}%`,
              } as CSSProperties
            }
          />
        )}
        {total === 0 ? (
          <span className="outline-luminous-empty">暂无章节</span>
        ) : (
          entries.map((entry, index) => {
            const showCaption = captionIndex === index;
            return (
              <button
                key={`${entry.pos}-${entry.text}`}
                type="button"
                data-tick-index={index}
                className={cn(
                  "outline-luminous-tick",
                  `outline-luminous-tick--level-${entry.level}`,
                  index === activeIndex && "outline-luminous-tick--active",
                  index === hoverIndex && "outline-luminous-tick--hovered",
                  index === focusIndex && "outline-luminous-tick--focused",
                )}
                style={
                  {
                    "--outline-tick-top": `${getTickTop(index, total)}%`,
                  } as CSSProperties
                }
                aria-label={entry.text}
                aria-current={index === activeIndex ? "location" : undefined}
                onClick={() => jumpTo(entry.pos)}
                onPointerEnter={() => {
                  setFocusIndex(null);
                  setHoverIndex(index);
                }}
                onFocus={() => {
                  setFocusIndex(index);
                  setHoverIndex(index);
                }}
              >
                {showCaption ? (
                  <OutlineLuminousCaption
                    entry={entry}
                    variant={captionVariant}
                  />
                ) : null}
              </button>
            );
          })
        )}
      </div>
    </nav>
  );
});
