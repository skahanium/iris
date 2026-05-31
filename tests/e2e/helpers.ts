/**
 * 统一助手 E2E 契约：选择器与流程断言（可在 Vitest 或未来 Playwright/Tauri 驱动中复用）。
 */

import { resolveAssistantIntent } from "@/lib/assistant-routing";

/** 与 `UnifiedAssistantPanel` / `AppShell` 上 `data-testid` 对齐 */
export const E2E_SELECTORS = {
  editor: '[data-testid="editor"]',
  editorShell: '[data-testid="editor-shell"]',
  tabBar: '[data-testid="desktop-title-bar"]',
  statusBar: '[data-testid="status-bar"]',
  assistantDock: '[data-testid="unified-assistant-dock"]',
  assistantPanel: '[data-testid="unified-assistant-panel"]',
  aiInput: '[data-testid="ai-input"]',
  aiMessages: '[data-testid="ai-message-list"]',
  executionPlan: '[data-testid="execution-plan-preview"]',
  researchFocus: '[data-testid="research-focus"]',
  patchPreview: '[data-testid="patch-preview"]',
} as const;

export type UnifiedAssistantFlow =
  | "selection_rewrite"
  | "mention_scope_lookup"
  | "web_knowledge_chat"
  | "citation_check"
  | "research_focus";

/**
 * 将用户场景映射到 `AssistantIntent`（与统一助手自动路由一致）。
 */
export function intentForFlow(flow: UnifiedAssistantFlow): string {
  switch (flow) {
    case "selection_rewrite":
      return resolveAssistantIntent({
        message: "帮我改写这段，让它更精炼",
        hasSelection: true,
        notePath: "notes/demo.md",
        explicitScope: false,
      });
    case "mention_scope_lookup":
      return resolveAssistantIntent({
        message: "查一下 @notes/demo.md 里的向量检索",
        hasSelection: false,
        notePath: "notes/demo.md",
        explicitScope: true,
      });
    case "web_knowledge_chat":
      return resolveAssistantIntent({
        message: "帮我查一下 SQLite 向量扩展相关的资料",
        hasSelection: false,
        notePath: null,
        explicitScope: false,
      });
    case "citation_check":
      return resolveAssistantIntent({
        message: "检查这一段的引用是否充分",
        hasSelection: true,
        notePath: "notes/demo.md",
        explicitScope: false,
      });
    case "research_focus":
      return resolveAssistantIntent({
        message: "研究一下 sqlite-vec 和 FTS5 在本地知识库中的取舍",
        hasSelection: false,
        notePath: null,
        explicitScope: true,
      });
  }
}

/** @deprecated 统一助手不再暴露场景选择器；保留空实现供旧测试迁移 */
export async function waitForAiPanel(): Promise<void> {
  return;
}

/** @deprecated 使用 `intentForFlow` + 自动路由，勿再选择 scene */
export async function selectAiScene(_scene: string): Promise<void> {
  return;
}

/** @deprecated 在 Tauri E2E 中应操作 `[data-testid="ai-input"]` */
export async function sendAiMessage(_message: string): Promise<void> {
  return;
}

/** @deprecated 在 Tauri E2E 中应检查 `ContextPacketDrawer` / 证据卡 */
export function expectContextPackets(_count: number): void {
  return;
}
