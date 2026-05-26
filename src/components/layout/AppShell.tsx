import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

interface AppShellProps {
  tabBar: ReactNode;
  editor: ReactNode;
  aiPanel: ReactNode;
  statusBar: ReactNode;
  aiPanelOpen?: boolean;
  overlays?: ReactNode;
}

export function AppShell({
  tabBar,
  editor,
  aiPanel,
  statusBar,
  aiPanelOpen = true,
  overlays,
}: AppShellProps) {
  return (
    <div className="flex h-dvh flex-col overflow-hidden bg-background">
      {tabBar}
      <div className="flex min-h-0 flex-1">
        <main className="relative flex min-w-0 flex-1 flex-col bg-background">
          {editor}
        </main>
        <aside
          className={cn(
            "flex shrink-0 flex-col border-l border-border bg-panel transition-[width] duration-200 ease-out",
            aiPanelOpen ? "w-[280px]" : "w-0 overflow-hidden border-transparent",
          )}
          aria-hidden={!aiPanelOpen}
        >
          <div
            className={cn(
              "flex h-full w-[280px] flex-col",
              !aiPanelOpen && "pointer-events-none opacity-0",
            )}
          >
            {aiPanel}
          </div>
        </aside>
      </div>
      {statusBar}
      {overlays}
    </div>
  );
}
