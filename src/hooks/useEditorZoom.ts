import { useCallback, useState } from "react";

import {
  clampEditorZoom,
  EDITOR_ZOOM_DEFAULT,
  loadEditorZoom,
  saveEditorZoom,
  stepEditorZoom,
} from "@/lib/editor-zoom";

export function useEditorZoom() {
  const [zoom, setZoomState] = useState(loadEditorZoom);

  const setZoom = useCallback((value: number) => {
    const next = clampEditorZoom(value);
    setZoomState(next);
    saveEditorZoom(next);
  }, []);

  const zoomIn = useCallback(() => {
    setZoomState((prev) => {
      const next = stepEditorZoom(prev, 1);
      saveEditorZoom(next);
      return next;
    });
  }, []);

  const zoomOut = useCallback(() => {
    setZoomState((prev) => {
      const next = stepEditorZoom(prev, -1);
      saveEditorZoom(next);
      return next;
    });
  }, []);

  const resetZoom = useCallback(() => {
    setZoom(EDITOR_ZOOM_DEFAULT);
  }, [setZoom]);

  return { zoom, setZoom, zoomIn, zoomOut, resetZoom };
}
