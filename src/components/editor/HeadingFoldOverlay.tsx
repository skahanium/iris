import type { Editor } from "@tiptap/react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { memo, useCallback, useEffect, useRef, useState } from "react";

import {
  collectFoldableHeadingBlocks,
  getHeadingFoldState,
  type FoldableHeadingBlock,
} from "./extensions/HeadingFoldExtension";

interface HeadingFoldOverlayProps {
  editor: Editor | null;
}

interface OverlayItem extends FoldableHeadingBlock {
  top: number;
}

function findScrollParent(element: HTMLElement | null): HTMLElement | Window {
  let current = element?.parentElement ?? null;
  while (current) {
    const { overflowY } = window.getComputedStyle(current);
    if (overflowY === "auto" || overflowY === "scroll") {
      return current;
    }
    current = current.parentElement;
  }
  return window;
}

export const HeadingFoldOverlay = memo(function HeadingFoldOverlay({
  editor,
}: HeadingFoldOverlayProps) {
  const overlayRef = useRef<HTMLDivElement | null>(null);
  const frameRef = useRef<number | null>(null);
  const [items, setItems] = useState<OverlayItem[]>([]);

  const measure = useCallback(() => {
    const host = overlayRef.current?.parentElement;
    if (!editor || editor.isDestroyed || !host) {
      setItems([]);
      return;
    }

    const foldState = getHeadingFoldState(editor.state);
    const collapsed = foldState?.collapsed ?? new Set<number>();
    const hostRect = host.getBoundingClientRect();
    const next = collectFoldableHeadingBlocks(editor.state.doc, collapsed)
      .map((item) => {
        const dom = editor.view.nodeDOM(item.pos);
        if (!(dom instanceof HTMLElement)) return null;
        const rect = dom.getBoundingClientRect();
        return {
          ...item,
          top: rect.top - hostRect.top + rect.height / 2,
        };
      })
      .filter((item): item is OverlayItem => item !== null);
    setItems(next);
  }, [editor]);

  const scheduleMeasure = useCallback(() => {
    if (frameRef.current !== null) {
      cancelAnimationFrame(frameRef.current);
    }
    frameRef.current = requestAnimationFrame(() => {
      frameRef.current = null;
      measure();
    });
  }, [measure]);

  useEffect(() => {
    scheduleMeasure();
  }, [scheduleMeasure]);

  useEffect(() => {
    if (!editor) {
      setItems([]);
      return;
    }

    const scrollParent = findScrollParent(editor.view.dom);
    editor.on("transaction", scheduleMeasure);
    window.addEventListener("resize", scheduleMeasure);
    scrollParent.addEventListener("scroll", scheduleMeasure, { passive: true });

    return () => {
      editor.off("transaction", scheduleMeasure);
      window.removeEventListener("resize", scheduleMeasure);
      scrollParent.removeEventListener("scroll", scheduleMeasure);
      if (frameRef.current !== null) {
        cancelAnimationFrame(frameRef.current);
        frameRef.current = null;
      }
    };
  }, [editor, scheduleMeasure]);

  if (!editor || items.length === 0) {
    return <div ref={overlayRef} className="iris-heading-fold-overlay" />;
  }

  return (
    <div ref={overlayRef} className="iris-heading-fold-overlay">
      {items.map((item) => (
        <span
          key={item.pos}
          className="iris-heading-fold-row"
          style={{ top: item.top }}
        >
          <button
            type="button"
            className="iris-heading-fold-btn"
            aria-label={item.collapsed ? "展开章节" : "折叠章节"}
            title={item.collapsed ? "展开章节" : "折叠章节"}
            onMouseDown={(event) => {
              event.preventDefault();
              event.stopPropagation();
            }}
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              editor.commands.toggleHeadingFold(item.pos);
              scheduleMeasure();
            }}
          >
            {item.collapsed ? (
              <ChevronRight className="h-3.5 w-3.5" aria-hidden />
            ) : (
              <ChevronDown className="h-3.5 w-3.5" aria-hidden />
            )}
          </button>
        </span>
      ))}
    </div>
  );
});
