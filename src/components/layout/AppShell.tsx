import type { ReactNode } from "react";
import { useCallback, useRef, useState } from "react";

import {
  AI_PANEL_WIDTH_DEFAULT,
  AI_PANEL_WIDTH_MAX,
  AI_PANEL_WIDTH_MIN,
  loadAiPanelWidth,
  saveAiPanelWidth,
} from "@/lib/ai-panel-width";
import { cn } from "@/lib/utils";

interface AppShellProps {
  tabBar: ReactNode;
  editor: ReactNode;
  aiPanel: ReactNode;
  statusBar: ReactNode;
  aiPanelOpen?: boolean;
  zen?: boolean;
  overlays?: ReactNode;
}

export function AppShell({
  tabBar,
  editor,
  aiPanel,
  statusBar,
  aiPanelOpen = true,
  zen = false,
  overlays,
}: AppShellProps) {
  const [panelWidth, setPanelWidth] = useState(loadAiPanelWidth);
  const [isResizing, setIsResizing] = useState(false);
  const dragRef = useRef<{ startX: number; startWidth: number } | null>(null);

  const clampWidth = useCallback((next: number) => {
    return Math.min(AI_PANEL_WIDTH_MAX, Math.max(AI_PANEL_WIDTH_MIN, next));
  }, []);

  const onResizePointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (!aiPanelOpen) return;
      e.preventDefault();
      setIsResizing(true);
      dragRef.current = { startX: e.clientX, startWidth: panelWidth };
      e.currentTarget.setPointerCapture(e.pointerId);

      const onMove = (ev: PointerEvent) => {
        const drag = dragRef.current;
        if (!drag) return;
        const delta = drag.startX - ev.clientX;
        setPanelWidth(clampWidth(drag.startWidth + delta));
      };

      const onUp = () => {
        dragRef.current = null;
        setIsResizing(false);
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
        setPanelWidth((w) => {
          saveAiPanelWidth(w);
          return w;
        });
      };

      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", onUp);
    },
    [aiPanelOpen, panelWidth, clampWidth],
  );

  const widthPx = aiPanelOpen ? panelWidth : 0;

  return (
    <div className="flex h-full min-h-0 flex-1 flex-col overflow-hidden bg-background">
      {!zen ? tabBar : null}
      <div className="flex min-h-0 flex-1">
        <main className="relative flex min-w-0 flex-1 flex-col bg-background">
          {editor}
        </main>
        {!zen ? (
          <aside
            data-testid="unified-assistant-dock"
            className={cn(
              "relative z-ai flex shrink-0 flex-col border-l border-border bg-panel",
              !isResizing && "transition-[width] duration-200 ease-out",
              !aiPanelOpen && "overflow-hidden border-transparent",
            )}
            style={{ width: widthPx }}
            aria-hidden={!aiPanelOpen}
          >
            {aiPanelOpen ? (
              <div
                role="separator"
                aria-orientation="vertical"
                aria-label="调整 AI 侧栏宽度"
                className="absolute left-0 top-0 z-10 h-full w-1.5 -translate-x-1/2 cursor-col-resize touch-none hover:bg-primary/20"
                onPointerDown={onResizePointerDown}
              />
            ) : null}
            <div
              className={cn(
                "flex h-full flex-col",
                !aiPanelOpen && "pointer-events-none opacity-0",
              )}
              style={{
                width: aiPanelOpen ? panelWidth : AI_PANEL_WIDTH_DEFAULT,
              }}
            >
              {aiPanel}
            </div>
          </aside>
        ) : null}
      </div>
      {!zen ? statusBar : null}
      {overlays}
    </div>
  );
}
