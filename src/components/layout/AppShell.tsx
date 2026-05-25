import type { ReactNode } from "react";

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
    <div className="flex h-screen flex-col overflow-hidden bg-background">
      {tabBar}
      <div className="flex min-h-0 flex-1">
        <main className="flex min-w-0 flex-1 flex-col bg-editor-paper">
          {editor}
        </main>
        <aside
          className={
            aiPanelOpen
              ? "w-[280px] shrink-0 border-l border-border bg-panel transition-[width] duration-200"
              : "w-0 shrink-0 overflow-hidden border-l border-transparent transition-[width] duration-200"
          }
          aria-hidden={!aiPanelOpen}
        >
          {aiPanel}
        </aside>
      </div>
      {statusBar}
      {overlays}
    </div>
  );
}
