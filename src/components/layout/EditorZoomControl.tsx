import { useEffect, useRef, useState } from "react";

import { formatEditorZoomPercent } from "@/lib/editor-zoom";
import { cn } from "@/lib/utils";

interface EditorZoomControlProps {
  editorZoom: number;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onZoomReset: () => void;
}

export function EditorZoomControl({
  editorZoom,
  onZoomIn,
  onZoomOut,
  onZoomReset,
}: EditorZoomControlProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [open]);

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        className="rounded-md px-2 py-0.5 text-[11px] tabular-nums text-muted-foreground transition-colors duration-base ease-iris-out hover:bg-surface-inset/80 hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-1 focus-visible:ring-offset-panel"
        aria-expanded={open}
        aria-haspopup="dialog"
        aria-label="编辑器缩放"
        onClick={() => setOpen((o) => !o)}
      >
        {formatEditorZoomPercent(editorZoom)}
      </button>
      {open ? (
        <div
          className="absolute bottom-full right-0 z-overlay mb-1 flex min-w-[10rem] items-center gap-1 rounded-lg border border-border/80 bg-surface-elevated p-1.5 shadow-floating"
          role="dialog"
          aria-label="缩放"
        >
          <button
            type="button"
            className="flex h-7 w-7 items-center justify-center rounded-md text-sm hover:bg-surface-inset"
            aria-label="缩小"
            onClick={onZoomOut}
          >
            −
          </button>
          <button
            type="button"
            className={cn(
              "min-w-[3rem] flex-1 rounded-md px-2 py-1 text-center text-[11px] tabular-nums",
              "hover:bg-surface-inset",
            )}
            onClick={onZoomReset}
          >
            重置
          </button>
          <button
            type="button"
            className="flex h-7 w-7 items-center justify-center rounded-md text-sm hover:bg-surface-inset"
            aria-label="放大"
            onClick={onZoomIn}
          >
            +
          </button>
        </div>
      ) : null}
    </div>
  );
}
