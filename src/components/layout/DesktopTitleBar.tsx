import { getCurrentWindow } from "@tauri-apps/api/window";
import { Plus, X } from "lucide-react";
import { memo, useMemo } from "react";

import { IrisMark } from "@/components/brand/IrisMark";
import { isMacOSDesktopChrome, showCustomWindowControls } from "@/lib/platform-chrome";
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

export type DesktopTitleBarVariant = "document" | "splash";

interface DesktopTitleBarProps {
  variant?: DesktopTitleBarVariant;
  tabs: TabItem[];
  activePath: string | null;
  onSelect: (path: string) => void;
  onClose: (path: string) => void;
  onNew: () => void;
}

export const DesktopTitleBar = memo(function DesktopTitleBar({
  variant = "document",
  tabs,
  activePath,
  onSelect,
  onClose,
  onNew,
}: DesktopTitleBarProps) {
  const isDesktop = isTauriRuntime();
  const isMacDesktop = isMacOSDesktopChrome();
  const isSplash = variant === "splash";
  const hasTabs = tabs.length > 0;
  const showBrandColumn = isDesktop && (isSplash || !hasTabs);
  const showTabStrip = !isSplash;
  /** macOS 无 Tab：品牌与「+」同一行垂直居中，与系统交通灯对齐 */
  const macEmptyToolbar = isMacDesktop && !isSplash && !hasTabs;
  /** macOS 窗口模式：整行 items-center，与 32px 交通灯中线对齐 */
  const macCenteredChrome = isMacDesktop && !isSplash;

  const onDragMouseDown = useMemo(() => {
    if (!isDesktop) return undefined;
    return createWindowDragMouseDown(getCurrentWindow());
  }, [isDesktop]);

  return (
    <header
      role="banner"
      data-testid="desktop-title-bar"
      className={cn(
        "iris-desktop-titlebar flex h-[var(--titlebar-height)] shrink-0 cursor-default select-none border-b border-border/60 bg-surface-chrome",
        macCenteredChrome ? "items-center" : "items-stretch",
        isDesktop && "iris-desktop-titlebar--desktop",
        macEmptyToolbar && "iris-desktop-titlebar--mac-empty",
      )}
      data-tauri-drag-region={isDesktop ? true : undefined}
      onMouseDown={onDragMouseDown}
    >
      {macEmptyToolbar ? (
        <>
          <div
            className="flex shrink-0 items-center gap-2 pl-1 pr-2"
            aria-label="拖动窗口"
          >
            <IrisMark size={18} />
            <span className="text-sm font-semibold tracking-tight text-foreground/90">
              Iris
            </span>
          </div>
          <div className="min-w-0 flex-1 self-stretch" data-tauri-drag-region />
          <button
            type="button"
            data-tauri-drag-region-exclude
            className="mr-1 inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors duration-fast hover:bg-muted/60 hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-primary"
            aria-label="新建笔记"
            onMouseDown={(event) => {
              event.stopPropagation();
            }}
            onClick={onNew}
          >
            <Plus className="h-4 w-4" />
          </button>
        </>
      ) : null}

      {!macEmptyToolbar && showBrandColumn ? (
        isSplash ? (
          <AppBrandZone className="min-w-0 flex-1 justify-start px-5" />
        ) : (
          <div
            className="flex h-full shrink-0 items-center gap-2 px-3"
            aria-label="拖动窗口"
          >
            <IrisMark size={18} />
            <span className="text-sm font-semibold tracking-tight text-foreground/90">
              Iris
            </span>
          </div>
        )
      ) : null}

      {!macEmptyToolbar && showTabStrip ? (
        <div className="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto px-1">
          {tabs.map((tab) => {
            const active = activePath === tab.path;
            return (
              <div
                key={tab.path}
                className="group flex max-w-[220px] shrink-0 items-center"
              >
                <button
                  type="button"
                  data-tauri-drag-region-exclude
                  className={cn(
                    "min-w-0 flex-1 truncate px-2.5 py-1.5 text-left text-xs transition-colors duration-fast focus:outline-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-1 focus-visible:ring-offset-panel",
                    active
                      ? "font-medium text-foreground shadow-[inset_0_-2px_0_0_hsl(var(--primary))]"
                      : "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
                  )}
                  title={tab.path !== tab.title ? tab.path : undefined}
                  onMouseDown={(event) => {
                    event.stopPropagation();
                  }}
                  onClick={() => onSelect(tab.path)}
                >
                  {tab.title}
                  {tab.dirty ? (
                    <span className="text-muted-foreground"> •</span>
                  ) : null}
                </button>
                <button
                  type="button"
                  data-tauri-drag-region-exclude
                  className={cn(
                    "flex w-8 shrink-0 items-center justify-center text-muted-foreground opacity-0 transition-opacity duration-fast hover:text-foreground focus:outline-none focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-primary group-hover:opacity-100",
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
          <div className="shrink-0" data-tauri-drag-region-exclude>
            <button
              type="button"
              data-tauri-drag-region-exclude
              className={cn(
                "inline-flex items-center justify-center rounded-md text-muted-foreground transition-colors duration-fast hover:bg-muted/60 hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-primary",
                macCenteredChrome ? "h-7 w-7" : "h-8 w-8",
              )}
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
      ) : !macEmptyToolbar ? (
        <div className="min-w-0 flex-1" data-tauri-drag-region />
      ) : null}

      {isDesktop && showCustomWindowControls() ? <WindowControls /> : null}
    </header>
  );
});
