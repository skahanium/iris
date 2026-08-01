import { createContext, useContext } from "react";

import type { WorkspaceChromeProjection } from "@/lib/workspace-chrome-layout";

/**
 * 布局动作出口：Agent 面板（AppAiPanelSlot 子树）通过它请求主区阅读或侧车打开。
 * AppShell 持有唯一布局策略实例并注入该 Context；面板不得自行切换 presentation。
 */
export interface WorkspaceChromeActions {
  /** 打开 Agent：预算允许侧车则打开侧车，否则进入主区阅读（§4.1 降级 4）。 */
  openAssistant: () => void;
  /** 进入 Agent 主区阅读（不持久化）。 */
  enterAssistantFocus: () => void;
  /** 返回文档主平面（不持久化）。 */
  exitAssistantFocus: () => void;
  /** 当前有效投影。 */
  projection: WorkspaceChromeProjection;
  /** 轻量导航打开意图（标题栏入口显示"打开/关闭笔记库导航"）。 */
  navigatorOpen: boolean;
  /** 用户是否偏好在宽度允许时固定导航。 */
  pinPreferred: boolean;
  /** 更新导航固定偏好；宽度不足时 presentation 仍自动降级为浮动抽屉。 */
  setPinPreferred: (preferred: boolean) => void;
  /** 切换轻量导航（Ctrl/Cmd+\ 与标题栏入口共用，不持久化）。 */
  toggleNavigator: () => void;
}

export const WorkspaceChromeActionsContext =
  createContext<WorkspaceChromeActions | null>(null);

export function useWorkspaceChromeActions(): WorkspaceChromeActions {
  const value = useContext(WorkspaceChromeActionsContext);
  if (!value) {
    throw new Error("useWorkspaceChromeActions 必须在 AppShell 内使用");
  }
  return value;
}
