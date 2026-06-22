import { Minus, Plus, RotateCcw } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import {
  EDITOR_ZOOM_MAX,
  EDITOR_ZOOM_MIN,
  EDITOR_ZOOM_STEP,
  formatEditorZoomPercent,
} from "@/lib/editor-zoom";
import { cn } from "@/lib/utils";

interface EditorZoomControlProps {
  editorZoom: number;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onZoomReset: () => void;
  onZoomChange: (zoom: number) => void;
}

export function EditorZoomControl({
  editorZoom,
  onZoomIn,
  onZoomOut,
  onZoomReset,
  onZoomChange,
}: EditorZoomControlProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const canZoomOut = editorZoom > EDITOR_ZOOM_MIN;
  const canZoomIn = editorZoom < EDITOR_ZOOM_MAX;

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
        className="iris-focus-soft rounded-md px-2 py-0.5 text-[11px] tabular-nums text-muted-foreground transition-[color,background-color,box-shadow] duration-base ease-iris-out hover:bg-surface-inset/80 hover:text-foreground focus:outline-none"
        aria-expanded={open}
        aria-haspopup="dialog"
        aria-label="编辑器缩放"
        onClick={() => setOpen((o) => !o)}
      >
        {formatEditorZoomPercent(editorZoom)}
      </button>
      {open ? (
        <div
          className="absolute bottom-full right-0 z-overlay mb-1 flex min-w-[13rem] items-center gap-2 rounded-lg border border-border/80 bg-surface-elevated p-2 shadow-floating"
          role="dialog"
          aria-label="缩放"
        >
          <button
            type="button"
            className="iris-focus-soft flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-surface-inset hover:text-foreground disabled:cursor-not-allowed disabled:opacity-35"
            aria-label="缩小"
            disabled={!canZoomOut}
            onClick={onZoomOut}
          >
            <Minus className="h-3.5 w-3.5" aria-hidden />
          </button>
          <div className="flex min-w-0 flex-1 flex-col gap-1">
            <input
              type="range"
              min={EDITOR_ZOOM_MIN}
              max={EDITOR_ZOOM_MAX}
              step={EDITOR_ZOOM_STEP}
              value={editorZoom}
              aria-label="缩放比例"
              className={cn(
                "h-1.5 w-full cursor-pointer appearance-none rounded-full bg-surface-inset accent-primary",
                "[&::-webkit-slider-thumb]:h-3 [&::-webkit-slider-thumb]:w-3",
              )}
              onChange={(event) =>
                onZoomChange(Number(event.currentTarget.value))
              }
            />
            <div className="flex items-center justify-between text-[10px] tabular-nums text-muted-foreground">
              <span>{formatEditorZoomPercent(EDITOR_ZOOM_MIN)}</span>
              <span className="font-medium text-foreground">
                {formatEditorZoomPercent(editorZoom)}
              </span>
              <span>{formatEditorZoomPercent(EDITOR_ZOOM_MAX)}</span>
            </div>
          </div>
          <button
            type="button"
            className="iris-focus-soft flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-surface-inset hover:text-foreground"
            aria-label="重置缩放"
            onClick={onZoomReset}
          >
            <RotateCcw className="h-3.5 w-3.5" aria-hidden />
          </button>
          <button
            type="button"
            className="iris-focus-soft flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-surface-inset hover:text-foreground disabled:cursor-not-allowed disabled:opacity-35"
            aria-label="放大"
            disabled={!canZoomIn}
            onClick={onZoomIn}
          >
            <Plus className="h-3.5 w-3.5" aria-hidden />
          </button>
        </div>
      ) : null}
    </div>
  );
}
