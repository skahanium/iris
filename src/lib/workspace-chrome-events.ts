/**
 * 工作区布局跨树事件（v1.2.19 Task 5）。
 *
 * 标题栏入口与快捷键（Ctrl/Cmd+\）位于 AppShell 的 WorkspaceChromeActionsContext
 * 之外，通过 window CustomEvent 请求布局切换，与 llm-events/prompt-profile 同模式。
 */
export const WORKSPACE_TOGGLE_NAVIGATOR_EVENT =
  "iris:workspace-toggle-navigator";

/** 请求切换轻量笔记库导航（AppShell 监听并执行，不携带任何笔记数据）。 */
export function requestWorkspaceNavigatorToggle(): void {
  window.dispatchEvent(new CustomEvent(WORKSPACE_TOGGLE_NAVIGATOR_EVENT));
}
