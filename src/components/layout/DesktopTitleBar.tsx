import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  FileImage,
  Lock,
  MoreHorizontal,
  Plus,
  Sparkles,
  X,
} from "lucide-react";
import {
  memo,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { IrisMark } from "@/components/brand/IrisMark";
import {
  IrisSurfaceMenuItem,
  IrisSurfaceMenuPanel,
} from "@/components/ui/iris-surface-menu";
import {
  isMacOSDesktopChrome,
  showCustomWindowControls,
} from "@/lib/platform-chrome";
import { isTauriRuntime } from "@/lib/tauri-runtime";
import { computeVisibleTabCount } from "@/lib/tab-overflow";
import { createWindowDragMouseDown } from "@/lib/window-drag";
import { cn } from "@/lib/utils";

import { AppBrandZone } from "./AppBrandZone";
import { WindowControls } from "./WindowControls";

export interface TabItem {
  /** Stable for the lifetime of an open document, even when its path changes. */
  documentSessionId?: string;
  path: string;
  title: string;
  dirty?: boolean;
  locked?: boolean;
  kind?: "note" | "media" | "artifact";
  /** A newly created disk-backed note may be discarded only before user intent promotes it. */
  lifecycle?: "session_pristine" | "persisted";
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

const TAB_MIN_PX = 72;
const TAB_GAP_PX = 4;
const MORE_BUTTON_PX = 32;
const NEW_BUTTON_PX = 32;

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
  const showTabStrip = !isSplash;

  const customWindowControls = isDesktop && showCustomWindowControls();
  /** Win/Linux 自定义三键时不要在 header 根上设 drag-region，否则 WebView2 会吞掉最小化/最大化点击。 */
  const headerNativeDragRegion = isDesktop && !customWindowControls;

  const onDragMouseDown = useMemo(() => {
    if (!isDesktop) return undefined;
    return createWindowDragMouseDown(getCurrentWindow());
  }, [isDesktop]);

  const railRef = useRef<HTMLDivElement>(null);
  const moreWrapRef = useRef<HTMLDivElement>(null);
  const [railWidth, setRailWidth] = useState(0);
  const [measuring, setMeasuring] = useState(true);
  const [compressed, setCompressed] = useState(false);
  const [visibleCount, setVisibleCount] = useState(tabs.length);
  const [moreOpen, setMoreOpen] = useState(false);

  useEffect(() => {
    const rail = railRef.current;
    if (!rail) return;
    const update = () => setRailWidth(rail.clientWidth);
    update();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(update);
    observer.observe(rail);
    return () => observer.disconnect();
  }, []);

  // A change to the tab set or the rail width requests a fresh measuring pass
  // that renders every tab at natural width so scrollWidth reflects the true
  // total before deciding whether to compress and spill into the 更多 menu.
  useLayoutEffect(() => {
    setMeasuring(true);
  }, [tabs, railWidth]);

  useLayoutEffect(() => {
    if (!measuring || railWidth <= 0) return;
    const rail = railRef.current;
    if (!rail) return;
    const naturalWidth = rail.scrollWidth;
    const available = rail.clientWidth;
    if (naturalWidth <= available) {
      setCompressed(false);
      setVisibleCount(tabs.length);
    } else {
      setCompressed(true);
      setVisibleCount(
        computeVisibleTabCount({
          gapPx: TAB_GAP_PX,
          moreButtonPx: MORE_BUTTON_PX,
          trailingButtonPx: NEW_BUTTON_PX,
          railWidthPx: available,
          tabCount: tabs.length,
          tabMinPx: TAB_MIN_PX,
        }),
      );
    }
    setMeasuring(false);
  }, [measuring, railWidth, tabs]);

  useEffect(() => {
    if (!moreOpen) return;
    const onDocClick = (event: MouseEvent) => {
      if (
        moreWrapRef.current &&
        !moreWrapRef.current.contains(event.target as Node)
      ) {
        setMoreOpen(false);
      }
    };
    document.addEventListener("mousedown", onDocClick);
    return () => document.removeEventListener("mousedown", onDocClick);
  }, [moreOpen]);

  const showCompressed = !measuring && compressed;
  const activeIdx = tabs.findIndex((tab) => tab.path === activePath);
  const overflow = !measuring && visibleCount < tabs.length;
  const safeCount = Math.max(1, visibleCount);

  const visibleIndices = useMemo<number[]>(() => {
    if (measuring || !overflow) return tabs.map((_, index) => index);
    if (activeIdx >= 0 && activeIdx >= safeCount) {
      return [
        ...Array.from({ length: safeCount - 1 }, (_, index) => index),
        activeIdx,
      ];
    }
    return Array.from({ length: safeCount }, (_, index) => index);
  }, [activeIdx, measuring, overflow, safeCount, tabs]);

  const visibleTabs = visibleIndices
    .map((index) => tabs[index])
    .filter((tab): tab is TabItem => tab != null);
  const overflowTabs = tabs.filter(
    (_, index) => !visibleIndices.includes(index),
  );

  const renderTabSegment = (tab: TabItem) => {
    const active = activePath === tab.path;
    const isArtifact = tab.kind === "artifact";
    const isMedia = tab.kind === "media";
    return (
      <div
        key={tab.path}
        data-testid="rail-segment-tab"
        data-tauri-drag-region-exclude
        className={cn(
          "iris-focus-soft-within iris-rail-tab group flex min-w-0 items-center gap-1 px-2 py-1 text-xs transition-[background-color,color,box-shadow] duration-fast",
          showCompressed && "iris-rail-tab--compressed",
          active
            ? "iris-rail-tab--active text-[hsl(var(--outline-rail-active))]"
            : "text-muted-foreground hover:bg-[hsl(var(--outline-rail-active)/0.06)]",
        )}
        title={
          isArtifact ? tab.title : tab.path !== tab.title ? tab.path : undefined
        }
      >
        <button
          type="button"
          data-tauri-drag-region-exclude
          className="flex min-w-0 flex-1 items-center overflow-hidden rounded-md px-1 py-1 text-left focus:outline-none"
          onMouseDown={(event) => {
            event.stopPropagation();
          }}
          onClick={() => {
            onSelect(tab.path);
          }}
        >
          {tab.locked ? (
            <Lock className="mr-1 h-3 w-3 shrink-0 text-muted-foreground/70" />
          ) : null}
          {isArtifact ? (
            <Sparkles className="mr-1 h-3 w-3 shrink-0 text-muted-foreground/70" />
          ) : null}
          {isMedia ? (
            <FileImage className="mr-1 h-3 w-3 shrink-0 text-muted-foreground/70" />
          ) : null}
          <span className="min-w-0 truncate">{tab.title}</span>
          {isArtifact ? (
            <span className="ml-1 shrink-0 rounded-sm border border-border-subtle px-1 text-micro text-muted-foreground">
              临时
            </span>
          ) : null}
          {tab.dirty ? (
            <span className="shrink-0 text-muted-foreground"> • </span>
          ) : null}
        </button>
        <button
          type="button"
          data-tauri-drag-region-exclude
          className={cn(
            "iris-focus-soft flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-muted-foreground opacity-0 transition-all duration-fast hover:bg-muted/60 hover:text-foreground focus:outline-none focus-visible:opacity-100 group-hover:opacity-100",
            active && "opacity-60",
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
  };

  return (
    <header
      role="banner"
      data-testid="desktop-title-bar"
      className={cn(
        "iris-desktop-titlebar flex h-[var(--titlebar-height)] shrink-0 cursor-default select-none border-b border-border-subtle bg-surface-chrome",
        "items-center pl-[var(--titlebar-leading-inset)]",
        isDesktop && "iris-desktop-titlebar--desktop",
        customWindowControls && "relative pr-[var(--window-controls-width)]",
      )}
      data-tauri-drag-region={headerNativeDragRegion ? true : undefined}
      onMouseDown={onDragMouseDown}
    >
      {isSplash ? (
        <>
          {isMacDesktop ? (
            <div
              aria-hidden="true"
              className="iris-titlebar-traffic-spacer h-full shrink-0"
              data-tauri-drag-region
              style={{ width: "var(--titlebar-traffic-inset)" }}
            />
          ) : null}
          <AppBrandZone className="min-w-0 flex-1 justify-start px-5" />
        </>
      ) : (
        <>
          {isMacDesktop ? (
            <div
              aria-hidden="true"
              className="iris-titlebar-traffic-spacer h-full shrink-0"
              data-tauri-drag-region
              style={{ width: "var(--titlebar-traffic-inset)" }}
            />
          ) : null}

          {isDesktop ? (
            <div
              data-testid="iris-brand-rail"
              data-tauri-drag-region
              className="iris-brand-rail flex h-8 min-w-[6.75rem] shrink-0 select-none items-center justify-center gap-2 px-3 text-foreground"
            >
              <IrisMark size={18} />
              <span className="text-sm font-semibold">Iris</span>
            </div>
          ) : null}

          {showTabStrip ? (
            <>
              <div
                ref={railRef}
                className="iris-titlebar-tab-rail flex min-w-0 flex-1 items-center gap-1 overflow-x-hidden px-2"
                data-tauri-drag-region={customWindowControls ? true : undefined}
              >
                {visibleTabs.map(renderTabSegment)}
                {overflow ? (
                  <div
                    ref={moreWrapRef}
                    className="relative shrink-0"
                    data-tauri-drag-region-exclude
                  >
                    <button
                      type="button"
                      data-tauri-drag-region-exclude
                      className="iris-focus-soft inline-flex h-8 w-8 items-center justify-center rounded-[10px] text-muted-foreground transition-all duration-fast hover:bg-muted/60 hover:text-foreground focus:outline-none"
                      aria-label="更多笔记"
                      aria-expanded={moreOpen}
                      onMouseDown={(event) => event.stopPropagation()}
                      onClick={() => setMoreOpen((value) => !value)}
                    >
                      <MoreHorizontal className="h-4 w-4" />
                    </button>
                    {moreOpen ? (
                      <div
                        className="absolute right-0 top-full z-50 mt-1 min-w-[12rem]"
                        data-testid="rail-overflow-menu"
                      >
                        <IrisSurfaceMenuPanel aria-label="更多笔记">
                          {overflowTabs.map((tab) => (
                            <IrisSurfaceMenuItem
                              key={tab.path}
                              id={tab.path}
                              label={tab.title}
                              active={activePath === tab.path}
                              onSelect={() => {
                                onSelect(tab.path);
                                setMoreOpen(false);
                              }}
                            />
                          ))}
                        </IrisSurfaceMenuPanel>
                      </div>
                    ) : null}
                  </div>
                ) : null}
                <button
                  type="button"
                  data-testid="rail-new-note-button"
                  data-tauri-drag-region-exclude
                  className="iris-focus-soft inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-[10px] text-muted-foreground transition-all duration-fast hover:bg-muted/60 hover:text-foreground focus:outline-none"
                  aria-label="新建笔记"
                  onMouseDown={(event) => {
                    event.stopPropagation();
                  }}
                  onClick={onNew}
                >
                  <Plus className="h-4 w-4" />
                </button>
              </div>
            </>
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

DesktopTitleBar.displayName = "DesktopTitleBar";
