import { getCurrentWindow } from "@tauri-apps/api/window";
import { Lock, Plus, X } from "lucide-react";
import { memo, useMemo } from "react";

import { IrisMark } from "@/components/brand/IrisMark";
import {
  isMacOSDesktopChrome,
  showCustomWindowControls,
} from "@/lib/platform-chrome";
import { isTauriRuntime } from "@/lib/tauri-runtime";
import { createWindowDragMouseDown } from "@/lib/window-drag";
import { cn } from "@/lib/utils";

import { AppBrandZone } from "./AppBrandZone";
import { WindowControls } from "./WindowControls";

export interface TabItem {
  path: string;
  title: string;
  dirty?: boolean;
  locked?: boolean;
}

export type DesktopTitleBarVariant = "document" | "splash";

interface DesktopTitleBarProps {
  variant?: DesktopTitleBarVariant;
  tabs: TabItem[];
  activePath: string | null;
  isHomeActive?: boolean;
  onHome?: () => void;
  onSelect: (path: string) => void;
  onClose: (path: string) => void;
  onNew: () => void;
}

export const DesktopTitleBar = memo(function DesktopTitleBar({
  variant = "document",
  tabs,
  activePath,
  isHomeActive = false,
  onHome,
  onSelect,
  onClose,
  onNew,
}: DesktopTitleBarProps) {
  const isDesktop = isTauriRuntime();
  const isMacDesktop = isMacOSDesktopChrome();
  const isSplash = variant === "splash";
  const showTabStrip = !isSplash;
  /** macOS 窗口模式：整行 items-center，与 32px 顶栏中线对齐 */
  const macCenteredChrome = isMacDesktop && !isSplash;

  const customWindowControls = isDesktop && showCustomWindowControls();
  /** Win/Linux 自定义三键时勿在 header 根上设 drag-region，否则 WebView2 会吞掉最小化/最大化点击 */
  const headerNativeDragRegion = isDesktop && !customWindowControls;

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
        customWindowControls && "relative pr-[var(--window-controls-width)]",
      )}
      data-tauri-drag-region={headerNativeDragRegion ? true : undefined}
      onMouseDown={onDragMouseDown}
    >
      {isSplash ? (
        <AppBrandZone className="min-w-0 flex-1 justify-start px-5" />
      ) : (
        <>
          {isDesktop ? (
            <button
              type="button"
              data-testid="iris-brand-rail"
              data-tauri-drag-region-exclude
              className={cn(
                "iris-brand-rail flex h-full shrink-0 items-center gap-2 border-r border-border/70 px-3 text-foreground",
                isHomeActive && "iris-brand-rail--active",
              )}
              aria-label="回到 Home"
              onMouseDown={(event) => event.stopPropagation()}
              onClick={onHome}
            >
              <IrisMark size={18} />
              <span className="text-sm font-semibold">Iris</span>
            </button>
          ) : null}

          {isHomeActive ? (
            <div
              data-testid="home-segment"
              className="iris-home-segment flex shrink-0 items-center px-2.5 py-1.5 text-xs font-medium"
            >
              Home
            </div>
          ) : null}

          {showTabStrip ? (
            <div
              className="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto px-1"
              data-tauri-drag-region={customWindowControls ? true : undefined}
            >
              {tabs.map((tab) => {
                const active = activePath === tab.path;
                return (
                  <div
                    key={tab.path}
                    className="group flex min-w-[7rem] max-w-[14rem] shrink-0 items-center"
                  >
                    <button
                      type="button"
                      data-testid="rail-segment-tab"
                      data-tauri-drag-region-exclude
                      className={cn(
                        "iris-rail-tab min-w-0 flex-1 truncate px-2.5 py-1.5 text-left text-xs transition-colors duration-fast focus:outline-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-1 focus-visible:ring-offset-panel",
                        active
                          ? "iris-rail-tab--active font-medium text-foreground"
                          : "text-muted-foreground hover:bg-rail-hover hover:text-foreground",
                      )}
                      title={tab.path !== tab.title ? tab.path : undefined}
                      onMouseDown={(event) => {
                        event.stopPropagation();
                      }}
                      onClick={() => onSelect(tab.path)}
                    >
                      {tab.locked ? (
                        <Lock className="mr-1 inline h-3 w-3 text-muted-foreground/70" />
                      ) : null}
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
          ) : (
            <div
              className="min-w-0 flex-1"
              data-tauri-drag-region={customWindowControls ? true : undefined}
            />
          )}
        </>
      )}

      {customWindowControls ? (
        <div className="absolute inset-y-0 right-0 z-30 flex">
          <WindowControls />
        </div>
      ) : null}
    </header>
  );
});
