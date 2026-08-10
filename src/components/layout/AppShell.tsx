import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";

import { WorkspaceChromeActionsContext } from "@/hooks/useWorkspaceChromeActions";
import { useWorkspaceChromeLayout } from "@/hooks/useWorkspaceChromeLayout";
import { WORKSPACE_TOGGLE_NAVIGATOR_EVENT } from "@/lib/workspace-chrome-events";
import { cn } from "@/lib/utils";
import type { WorkspacePrimarySurface } from "@/lib/workspace-chrome-layout";

interface AppShellProps {
  tabBar: ReactNode;
  editor: ReactNode;
  aiPanel: ReactNode;
  statusBar: ReactNode;
  /** 工作区导航子树（Task 7 提供）；壳层只负责 closed/peek/pinned placement。 */
  navigator?: ReactNode;
  /** 用户是否希望 Agent 侧车开启（受控意图；resize 不经过此通道，不会改写）。 */
  aiPanelOpen?: boolean;
  onAiPanelOpenChange?: (open: boolean) => void;
  onAssistantVisibilityChange?: (visible: boolean) => void;
  /** 用户是否希望文件导航打开（受控意图；Task 5/7 接线）。 */
  navigatorOpen?: boolean;
  /** 用户固定偏好（受控意图；Task 7 接线）。 */
  pinPreferred?: boolean;
  /** 用户主平面意图（受控；Task 4 接线）。 */
  primarySurface?: WorkspacePrimarySurface;
  onPrimarySurfaceChange?: (surface: WorkspacePrimarySurface) => void;
  zen?: boolean;
  overlays?: ReactNode;
}

type NavigatorTransitionState = "closed" | "visible" | "exiting";

function prefersReducedMotion(): boolean {
  return (
    window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false
  );
}

export function AppShell({
  tabBar,
  editor,
  aiPanel,
  statusBar,
  navigator,
  aiPanelOpen,
  onAiPanelOpenChange,
  onAssistantVisibilityChange,
  navigatorOpen,
  pinPreferred,
  primarySurface,
  onPrimarySurfaceChange,
  zen = false,
  overlays,
}: AppShellProps) {
  const [isResizing, setIsResizing] = useState(false);
  const dragRef = useRef<{ startX: number; startWidth: number } | null>(null);

  const layout = useWorkspaceChromeLayout({
    zenMode: zen,
    initialAiPanelOpen: aiPanelOpen ?? true,
  });
  const {
    budgets,
    containerRef,
    contentWidthPx,
    enterAssistantFocus,
    exitAssistantFocus,
    projection,
    setAiPanelOpen,
    setNavigatorOpen,
    setPinPreferred,
    setSidecarWidth,
    pinPreferred: layoutPinPreferred,
  } = layout;
  // 有效主平面：禅模式临时显示文档（§5.1），不覆盖 assistant_focus 意图。
  const mainHidden = !zen && projection.primarySurface === "assistant_focus";
  const navigatorRequested =
    navigator !== null &&
    navigator !== undefined &&
    projection.navigator !== "closed";
  const [navigatorTransition, setNavigatorTransition] =
    useState<NavigatorTransitionState>(() =>
      navigatorRequested ? "visible" : "closed",
    );
  const navigatorRequestedRef = useRef(navigatorRequested);
  navigatorRequestedRef.current = navigatorRequested;

  useEffect(() => {
    if (navigatorRequested) {
      setNavigatorTransition("visible");
      return;
    }
    setNavigatorTransition((current) => {
      if (current === "closed") return current;
      return zen || prefersReducedMotion() ? "closed" : "exiting";
    });
  }, [navigatorRequested, zen]);

  const handleNavigatorAnimationEnd = useCallback(
    (event: React.AnimationEvent<HTMLDivElement>) => {
      if (event.target !== event.currentTarget) return;
      if (event.animationName !== "iris-fade-out") return;
      if (navigatorTransition === "exiting" && !navigatorRequestedRef.current) {
        setNavigatorTransition("closed");
      }
    },
    [navigatorTransition],
  );

  // 受控意图同步：外部状态（快捷键/标题栏/面板动作）变化时写入布局策略；
  // resize 只改实测尺寸，不经过这些通道，因此不会改写用户意图。
  const prevAiPanelOpenRef = useRef(aiPanelOpen ?? true);
  useEffect(() => {
    const next = aiPanelOpen ?? true;
    const prev = prevAiPanelOpenRef.current;
    prevAiPanelOpenRef.current = next;
    if (next !== prev && !zen) {
      if (next) {
        // 用户主动打开 Agent：预算允许侧车则侧车，否则进入主区阅读（§4.1 降级 4）。
        const canHostSidecar =
          contentWidthPx >= budgets.documentProtectedPx + budgets.agentMinPx;
        if (!canHostSidecar) enterAssistantFocus();
      } else if (projection.primarySurface === "assistant_focus") {
        // 用户关闭侧车意图：先退出主区阅读。
        exitAssistantFocus();
      }
    }
    setAiPanelOpen(next);
  }, [
    aiPanelOpen,
    budgets,
    contentWidthPx,
    enterAssistantFocus,
    exitAssistantFocus,
    projection.primarySurface,
    setAiPanelOpen,
    zen,
  ]);
  useEffect(() => {
    if (navigatorOpen !== undefined) setNavigatorOpen(navigatorOpen);
  }, [navigatorOpen, setNavigatorOpen]);
  useEffect(() => {
    onAssistantVisibilityChange?.(!zen && projection.assistant !== "collapsed");
  }, [onAssistantVisibilityChange, projection.assistant, zen]);
  useEffect(() => {
    if (pinPreferred !== undefined) setPinPreferred(pinPreferred);
  }, [pinPreferred, setPinPreferred]);
  useEffect(() => {
    if (primarySurface === "assistant_focus") {
      enterAssistantFocus();
    } else if (primarySurface !== undefined) {
      exitAssistantFocus();
    }
  }, [primarySurface, enterAssistantFocus, exitAssistantFocus]);

  const requestAiPanelOpen = useCallback(
    (open: boolean) => {
      setAiPanelOpen(open);
      onAiPanelOpenChange?.(open);
    },
    [onAiPanelOpenChange, setAiPanelOpen],
  );

  const requestPrimarySurface = useCallback(
    (surface: WorkspacePrimarySurface) => {
      if (surface === "assistant_focus") {
        enterAssistantFocus();
      } else {
        exitAssistantFocus();
      }
      onPrimarySurfaceChange?.(surface);
    },
    [enterAssistantFocus, exitAssistantFocus, onPrimarySurfaceChange],
  );

  const openAssistant = useCallback(() => {
    if (projection.primarySurface === "assistant_focus") return;
    if (projection.assistant === "sidecar") return;
    const canHostSidecar =
      contentWidthPx >= budgets.documentProtectedPx + budgets.agentMinPx;
    if (canHostSidecar) {
      // 投影会按预算决定侧车宽度（可能收缩），不会突破文档保护宽度。
      requestAiPanelOpen(true);
    } else {
      // 空间不足：进入主区阅读，而不是继续压窄正文（§4.1 降级 4）。
      requestPrimarySurface("assistant_focus");
    }
  }, [
    budgets,
    contentWidthPx,
    projection.assistant,
    projection.primarySurface,
    requestAiPanelOpen,
    requestPrimarySurface,
  ]);

  const toggleNavigator = useCallback(() => {
    setNavigatorOpen(!layout.navigatorOpen);
  }, [layout.navigatorOpen, setNavigatorOpen]);

  // Ctrl/Cmd+\ 与标题栏入口共用同一动作：标题栏在 Context 外，经 window 事件转发。
  useEffect(() => {
    window.addEventListener(WORKSPACE_TOGGLE_NAVIGATOR_EVENT, toggleNavigator);
    return () =>
      window.removeEventListener(
        WORKSPACE_TOGGLE_NAVIGATOR_EVENT,
        toggleNavigator,
      );
  }, [toggleNavigator]);

  const chromeActions = useMemo(
    () => ({
      openAssistant,
      enterAssistantFocus: () => requestPrimarySurface("assistant_focus"),
      exitAssistantFocus: () => requestPrimarySurface("document"),
      projection,
      navigatorOpen: layout.navigatorOpen,
      pinPreferred: layoutPinPreferred,
      setPinPreferred,
      toggleNavigator,
    }),
    [
      layout.navigatorOpen,
      layoutPinPreferred,
      openAssistant,
      projection,
      requestPrimarySurface,
      setPinPreferred,
      toggleNavigator,
    ],
  );

  const onResizePointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (projection.assistant !== "sidecar") return;
      e.preventDefault();
      setIsResizing(true);
      dragRef.current = {
        startX: e.clientX,
        startWidth: projection.sidecarWidthPx,
      };
      e.currentTarget.setPointerCapture(e.pointerId);

      const onMove = (ev: PointerEvent) => {
        const drag = dragRef.current;
        if (!drag) return;
        setSidecarWidth(drag.startWidth + (drag.startX - ev.clientX));
      };

      const onUp = () => {
        dragRef.current = null;
        setIsResizing(false);
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
      };

      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", onUp);
    },
    [projection.assistant, projection.sidecarWidthPx, setSidecarWidth],
  );

  const handleDocumentSurfacePointerDownCapture = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      // 主区阅读中点击文档工作集（Tab、Quick Open 结果、新建按钮、状态栏等）
      // 先退出 focus，再执行原动作（§7.1）；面板内点击不退出。
      if (!mainHidden || zen) return;
      const target = e.target as HTMLElement;
      if (target.closest('[data-testid="unified-assistant-dock"]')) return;
      exitAssistantFocus();
    },
    [exitAssistantFocus, mainHidden, zen],
  );

  const navigatorNode =
    navigator && navigatorTransition !== "closed" ? (
      <div
        data-testid="workspace-navigator"
        data-presentation={projection.navigator}
        data-transition={
          navigatorTransition === "exiting" ? "exiting" : "entering"
        }
        aria-hidden={
          mainHidden || navigatorTransition === "exiting" || undefined
        }
        className={cn(
          "h-full min-h-0 overflow-hidden border-r border-border-subtle",
          projection.navigator === "pinned"
            ? "relative z-navigator shrink-0"
            : "absolute inset-y-0 left-0 z-navigator-overlay bg-panel shadow-overlay",
          navigatorTransition === "exiting"
            ? "pointer-events-none motion-safe:animate-iris-fade-out motion-reduce:animate-none"
            : "motion-safe:animate-iris-fade-in motion-reduce:animate-none",
          mainHidden && "pointer-events-none invisible",
        )}
        style={{
          width:
            projection.navigator === "pinned"
              ? "18rem"
              : "min(18rem, calc(100% - 3rem))",
          // Keep the last keyframe until React unmounts after exit; without it,
          // the browser briefly restores opacity to 1 and the drawer flashes.
          animationFillMode: "both",
        }}
        onAnimationEnd={handleNavigatorAnimationEnd}
      >
        {navigator}
      </div>
    ) : null;

  return (
    <div
      className="flex h-full min-h-0 flex-1 flex-col overflow-hidden bg-background"
      onPointerDownCapture={handleDocumentSurfacePointerDownCapture}
    >
      <WorkspaceChromeActionsContext.Provider value={chromeActions}>
        {!zen ? tabBar : null}
        <div
          ref={containerRef}
          data-testid="workspace-content"
          className="relative flex min-h-0 flex-1"
        >
          {navigatorNode}
          <main
            data-testid="workspace-main"
            aria-hidden={mainHidden || undefined}
            className={cn(
              "relative flex min-w-0 flex-1 flex-col bg-background",
              mainHidden && "pointer-events-none invisible",
            )}
          >
            {editor}
          </main>
          <aside
            data-testid="unified-assistant-dock"
            data-presentation={projection.assistant}
            aria-hidden={projection.assistant === "collapsed" || undefined}
            className={cn(
              "relative flex shrink-0 flex-col border-l border-border bg-panel",
              !isResizing && "transition-[width] duration-200 ease-out",
              projection.assistant === "collapsed" &&
                "overflow-hidden border-transparent",
              projection.assistant === "focus" &&
                "absolute inset-0 z-workspace-focus",
            )}
            style={{
              width:
                projection.assistant === "sidecar"
                  ? projection.sidecarWidthPx
                  : projection.assistant === "focus"
                    ? undefined
                    : 0,
            }}
          >
            {projection.assistant === "sidecar" ? (
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
                projection.assistant === "collapsed" &&
                  "pointer-events-none opacity-0",
              )}
              style={{
                width:
                  projection.assistant === "sidecar"
                    ? projection.sidecarWidthPx
                    : undefined,
              }}
            >
              {aiPanel}
            </div>
          </aside>
        </div>
      </WorkspaceChromeActionsContext.Provider>
      {!zen ? statusBar : null}
      {overlays}
    </div>
  );
}
