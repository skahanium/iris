import type { ReactNode } from "react";

interface AppShellProps {
  tabBar: ReactNode;
  editor: ReactNode;
  aiPanel: ReactNode;
  statusBar: ReactNode;
  overlays?: ReactNode;
}

export function AppShell({
  tabBar,
  editor,
  aiPanel,
  statusBar,
  overlays,
}: AppShellProps) {
  return (
    <div className="flex h-screen flex-col overflow-hidden">
      {tabBar}
      <div className="flex min-h-0 flex-1">
        <main className="flex min-w-0 flex-1 flex-col">{editor}</main>
        <aside className="w-[280px] shrink-0 border-l border-border bg-panel">
          {aiPanel}
        </aside>
      </div>
      {statusBar}
      {overlays}
    </div>
  );
}
