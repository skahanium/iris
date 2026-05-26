import { Plus, X } from "lucide-react";
import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export interface TabItem {
  path: string;
  title: string;
  dirty?: boolean;
}

interface TabBarProps {
  tabs: TabItem[];
  activePath: string | null;
  onSelect: (path: string) => void;
  onClose: (path: string) => void;
  onNew: () => void;
  chromeActions?: ReactNode;
}

export function TabBar({
  tabs,
  activePath,
  onSelect,
  onClose,
  onNew,
  chromeActions,
}: TabBarProps) {
  return (
    <header className="flex h-10 shrink-0 items-stretch border-b border-border bg-panel">
      <div className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto px-2">
        {tabs.map((tab) => (
          <div
            key={tab.path}
            className={cn(
              "flex max-w-[220px] shrink-0 items-center rounded-xl border border-transparent transition-[background-color,box-shadow,border-color] duration-150",
              activePath === tab.path && "border-border/80 bg-card shadow-sm",
            )}
          >
            <button
              type="button"
              className={cn(
                "min-w-0 flex-1 truncate rounded-xl px-2.5 py-1.5 text-left text-xs transition-colors duration-150 focus:outline-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-1 focus-visible:ring-offset-panel",
                activePath === tab.path
                  ? "text-foreground"
                  : "text-muted-foreground hover:text-foreground",
              )}
              title={tab.path !== tab.title ? tab.path : undefined}
              onClick={() => onSelect(tab.path)}
            >
              {tab.dirty ? (
                <span className="text-primary" aria-hidden>
                  •{" "}
                </span>
              ) : null}
              {tab.title}
            </button>
            <button
              type="button"
              className="flex h-7 w-7 shrink-0 items-center justify-center rounded-xl text-muted-foreground transition-colors duration-150 hover:bg-muted hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-1 focus-visible:ring-offset-panel"
              aria-label={`关闭 ${tab.title}`}
              onClick={() => onClose(tab.path)}
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
        ))}
        <Button
          type="button"
          size="icon"
          variant="ghost"
          className="h-8 w-8 shrink-0 rounded-xl transition-colors duration-150"
          onClick={onNew}
          aria-label="新建笔记"
        >
          <Plus className="h-4 w-4" />
        </Button>
      </div>
      {chromeActions ? (
        <div className="flex shrink-0 items-center gap-1.5 border-l border-border px-2">
          {chromeActions}
        </div>
      ) : null}
    </header>
  );
}
