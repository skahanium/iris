import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { RefObject } from "react";

import {
  aiPanelWidthBounds,
  loadAiPanelWidth,
  saveAiPanelWidth,
} from "@/lib/ai-panel-width";
import {
  WORKSPACE_CHROME_DOCUMENT_PROTECTED_REM,
  computeWorkspaceChromeBudgets,
  loadNavigatorPinPreferred,
  projectWorkspaceChrome,
  saveNavigatorPinPreferred,
  type WorkspaceChromeBudgets,
  type WorkspaceChromeProjection,
  type WorkspacePrimarySurface,
} from "@/lib/workspace-chrome-layout";

const DEFAULT_ROOT_FONT_SIZE_PX = 16;

export interface UseWorkspaceChromeLayoutOptions {
  /** 禅模式临时覆盖：只改变有效 presentation，不覆盖用户意图。 */
  zenMode?: boolean;
  /** 初始 Agent 侧车开启意图（默认 true）。 */
  initialAiPanelOpen?: boolean;
  /** 初始导航打开意图（默认 false）。 */
  initialNavigatorOpen?: boolean;
  /** 初始固定偏好（默认读持久化值）。 */
  initialPinPreferred?: boolean;
}

export interface UseWorkspaceChromeLayoutResult {
  /** 挂载到工作区内容容器；ResizeObserver 从此读取实际内容宽度。 */
  containerRef: RefObject<HTMLDivElement | null>;
  contentWidthPx: number;
  rootFontSizePx: number;
  proseMeasurePx: number;
  budgets: WorkspaceChromeBudgets;
  /** 用户意图（resize 不自动改写）。 */
  aiPanelOpen: boolean;
  navigatorOpen: boolean;
  pinPreferred: boolean;
  primarySurface: WorkspacePrimarySurface;
  /** 用户保存的 Agent 宽度（已按当前根字号 clamp 到 25–45rem）。 */
  savedSidecarWidthPx: number;
  /** 有效 presentation 投影。 */
  projection: WorkspaceChromeProjection;
  /** 打开/关闭 Agent 侧车意图（不持久化）。 */
  setAiPanelOpen: (open: boolean) => void;
  /** 打开/关闭导航意图（不持久化；resize 不会自动重新打开）。 */
  setNavigatorOpen: (open: boolean) => void;
  /** 固定偏好（持久化）。 */
  setPinPreferred: (preferred: boolean) => void;
  /** 保存 Agent 宽度（按 rem 预算 clamp 并持久化）。 */
  setSidecarWidth: (widthPx: number) => void;
  /** 打开 Agent：预算允许侧车则打开侧车，否则进入 focus。 */
  openAssistant: () => void;
  /** 进入 Agent 主区阅读（不持久化）。 */
  enterAssistantFocus: () => void;
  /** 返回文档主平面（不持久化）。 */
  exitAssistantFocus: () => void;
}

function readRootFontSizePx(): number {
  if (typeof document === "undefined") return DEFAULT_ROOT_FONT_SIZE_PX;
  const value = parseFloat(getComputedStyle(document.documentElement).fontSize);
  return Number.isFinite(value) && value > 0
    ? value
    : DEFAULT_ROOT_FONT_SIZE_PX;
}

function readProseMeasurePx(rootFontSizePx: number): number {
  if (typeof document === "undefined") return 0;
  const raw = getComputedStyle(document.documentElement)
    .getPropertyValue("--prose-measure")
    .trim();
  const remValue = raw.endsWith("rem") ? parseFloat(raw) : NaN;
  if (!Number.isFinite(remValue) || remValue <= 0) {
    // CSS 缺失时回退常量 52rem，业务逻辑不假定固定 832px。
    return Math.round(WORKSPACE_CHROME_DOCUMENT_PROTECTED_REM * rootFontSizePx);
  }
  return Math.round(remValue * rootFontSizePx);
}

/**
 * 自适应工作区布局 hook（v1.2.19 Task 2）。
 *
 * 用 ResizeObserver 观察容器实际内容宽度与 documentElement（根字号/--prose-measure
 * 变化会触发尺寸回调），把实测尺寸与用户意图交给 projectWorkspaceChrome 做确定性投影。
 * 只持久化 Agent 宽度、导航固定偏好与安全目录展开标识（展开标识由导航器通过
 * workspace-chrome-layout 的 load/saveExpandedDirectories 消费）。
 */
export function useWorkspaceChromeLayout(
  options: UseWorkspaceChromeLayoutOptions = {},
): UseWorkspaceChromeLayoutResult {
  const zenMode = options.zenMode ?? false;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [contentWidthPx, setContentWidthPx] = useState(0);
  const [rootFontSizePx, setRootFontSizePx] = useState(readRootFontSizePx);
  const [proseMeasurePx, setProseMeasurePx] = useState(() =>
    readProseMeasurePx(rootFontSizePx),
  );
  const [aiPanelOpen, setAiPanelOpen] = useState(
    options.initialAiPanelOpen ?? true,
  );
  const [navigatorOpen, setNavigatorOpen] = useState(
    options.initialNavigatorOpen ?? false,
  );
  const [pinPreferred, setPinPreferredState] = useState(
    options.initialPinPreferred ?? loadNavigatorPinPreferred,
  );
  const [primarySurface, setPrimarySurface] =
    useState<WorkspacePrimarySurface>("document");
  const [savedSidecarWidthPx, setSavedSidecarWidthPx] =
    useState(loadAiPanelWidth);

  const budgets = useMemo(
    () => computeWorkspaceChromeBudgets(rootFontSizePx, proseMeasurePx),
    [rootFontSizePx, proseMeasurePx],
  );

  const projection = useMemo(
    () =>
      projectWorkspaceChrome({
        contentWidthPx,
        proseMeasurePx,
        rootFontSizePx,
        aiPanelOpen,
        navigatorOpen,
        pinPreferred,
        primarySurface,
        savedSidecarWidthPx,
        zenMode,
      }),
    [
      contentWidthPx,
      proseMeasurePx,
      rootFontSizePx,
      aiPanelOpen,
      navigatorOpen,
      pinPreferred,
      primarySurface,
      savedSidecarWidthPx,
      zenMode,
    ],
  );

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const refreshMetrics = () => {
      const root = readRootFontSizePx();
      setRootFontSizePx(root);
      setProseMeasurePx(readProseMeasurePx(root));
      setContentWidthPx(Math.round(container.getBoundingClientRect().width));
    };

    refreshMetrics();

    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", refreshMetrics);
      return () => window.removeEventListener("resize", refreshMetrics);
    }

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        if (entry.target === container) {
          setContentWidthPx(Math.round(entry.contentRect.width));
        }
      }
      // 根字号或 --prose-measure 变化会引发 rem 布局尺寸变化，借此重新读取。
      const root = readRootFontSizePx();
      setRootFontSizePx(root);
      setProseMeasurePx(readProseMeasurePx(root));
    });
    observer.observe(container);
    observer.observe(document.documentElement);
    return () => observer.disconnect();
  }, []);

  const setPinPreferred = useCallback((preferred: boolean) => {
    setPinPreferredState(preferred);
    saveNavigatorPinPreferred(preferred);
  }, []);

  const setSidecarWidth = useCallback(
    (widthPx: number) => {
      const bounds = aiPanelWidthBounds(rootFontSizePx);
      const clamped = Math.min(bounds.maxPx, Math.max(bounds.minPx, widthPx));
      setSavedSidecarWidthPx(clamped);
      saveAiPanelWidth(clamped, rootFontSizePx);
    },
    [rootFontSizePx],
  );

  const enterAssistantFocus = useCallback(() => {
    setPrimarySurface("assistant_focus");
  }, []);

  const exitAssistantFocus = useCallback(() => {
    setPrimarySurface("document");
  }, []);

  const openAssistant = useCallback(() => {
    if (primarySurface === "assistant_focus") return;
    if (aiPanelOpen && projection.assistant === "sidecar") return;
    const canHostSidecar =
      contentWidthPx >= budgets.documentProtectedPx + budgets.agentMinPx;
    if (canHostSidecar) {
      // 投影会按预算决定侧车宽度（可能收缩），不会突破文档保护宽度。
      setAiPanelOpen(true);
    } else {
      // 空间不足：进入主区阅读，而不是继续压窄正文（§4.1 降级 4）。
      setPrimarySurface("assistant_focus");
    }
  }, [
    aiPanelOpen,
    budgets,
    contentWidthPx,
    primarySurface,
    projection.assistant,
  ]);

  return {
    containerRef,
    contentWidthPx,
    rootFontSizePx,
    proseMeasurePx,
    budgets,
    aiPanelOpen,
    navigatorOpen,
    pinPreferred,
    primarySurface,
    savedSidecarWidthPx,
    projection,
    setAiPanelOpen,
    setNavigatorOpen,
    setPinPreferred,
    setSidecarWidth,
    openAssistant,
    enterAssistantFocus,
    exitAssistantFocus,
  };
}
