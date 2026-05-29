import { getCurrentWindow } from "@tauri-apps/api/window";
import { Plus, X } from "lucide-react";
import { memo, useMemo } from "react";

import { isTauriRuntime } from "@/lib/tauri-runtime";
import { createWindowDragMouseDown } from "@/lib/window-drag";
import { cn } from "@/lib/utils";

import { AppBrandZone } from "./AppBrandZone";
import { WindowControls } from "./WindowControls";

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
}

export const TabBar = memo(function TabBar({
  tabs,
  activePath,
  onSelect,
  onClose,
  onNew,
}: TabBarProps) {
  const showWindowControls = isTauriRuntime();
  const onDragMouseDown = useMemo(() => {
    if (!showWindowControls) return undefined;
    return createWindowDragMouseDown(getCurrentWindow());
  }, [showWindowControls]);

  return (
    <header
      data-testid="tab-bar"
      className="flex h-9 shrink-0 cursor-default select-none items-stretch border-b border-border/60 bg-surface-chrome"
      data-tauri-drag-region={showWindowControls ? true : undefined}
      onMouseDown={onDragMouseDown}
    >
      {showWindowControls ? <AppBrandZone /> : null}

      <div className="flex min-w-0 flex-1 items-end gap-0.5 overflow-x-auto px-1">
        {tabs.map((tab) => {
          const active = activePath === tab.path;
          return (
            <div
              key={tab.path}
              className={cn(
                "group flex max-w-[220px] shrink-0 items-stretch border-b-2 border-transparent",
                active && "border-primary",
              )}
            >
              <button
                type="button"
                data-tauri-drag-region-exclude
                className={cn(
                  "min-w-0 flex-1 truncate px-2.5 py-1.5 text-left text-xs transition-colors duration-fast focus:outline-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-1 focus-visible:ring-offset-panel",
                  active
                    ? "font-medium text-foreground"
                    : "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
                )}
                title={tab.path !== tab.title ? tab.path : undefined}
                onMouseDown={(event) => {
                  event.stopPropagation();
                }}
                onClick={() => onSelect(tab.path)}
              >
                {tab.title}
              </button>
              <button
                type="button"
                data-tauri-drag-region-exclude
                className={cn(
                  "flex w-7 shrink-0 items-center justify-center text-muted-foreground opacity-0 transition-opacity duration-fast hover:text-foreground focus:outline-none focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-primary group-hover:opacity-100",
                  active && "opacity-70",
                )}
                aria-label={`关闭 ${tab.title}`}
                onMouseDown={(event) => {
                  event.stopPropagation();
                }}
                onClick={(event) => {
                  event.stopPropagation();
                  onClose(tab.path);
                }}
              >
                <X className="h-3.5 w-3.5" />
              </button>
            </div>
          );
        })}
        <div className="mb-0.5 shrink-0" data-tauri-drag-region-exclude>
          <button
            type="button"
            data-tauri-drag-region-exclude
            className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors duration-fast hover:bg-muted/60 hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-primary"
            aria-label="新建笔记"
            onMouseDown={(event) => {
              event.stopPropagation();
            }}
            onClick={onNew}
          >
            <Plus className="h-4 w-4" />
          </button>
        </div>
      </div>

      {showWindowControls ? <WindowControls /> : null}
    </header>
  );
});
