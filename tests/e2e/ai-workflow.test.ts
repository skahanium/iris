/**
 * 统一助手工作流验收（契约 + 路由语义）。
 */
import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { intentForFlow } from "./helpers";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("统一助手工作流验收", () => {
  it("知识查阅：@ 范围 + 查库语义路由到 knowledge", () => {
    expect(intentForFlow("mention_scope_lookup")).toBe("knowledge");
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    expect(panel).toContain("parseMentionTokens");
    expect(panel).toContain("assistantExecute(");
  });

  it("选区改写：浮动工具条走内联流式建议", () => {
    expect(intentForFlow("selection_rewrite")).toBe("writing");
    const toolbar = read("src/components/editor/FloatingToolbar.tsx");
    expect(toolbar).toContain("onInlineAi");
    expect(toolbar).not.toContain("insertInlineAi");
    expect(read("src/components/editor/TipTapEditor.tsx")).toContain(
      "AiStreamExtension",
    );
  });

  it("引用检查：选区 + 检查语义进入 citation", () => {
    expect(intentForFlow("citation_check")).toBe("citation");
    expect(read("src/components/ai/UnifiedAssistantPanel.tsx")).toContain(
      "CitationCheckView",
    );
  });

  it("研究任务：专注态 UI 与中止", () => {
    expect(intentForFlow("research_focus")).toBe("research");
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    expect(panel).toContain('data-testid="research-focus"');
    expect(panel).toContain("ResearchFocusView");
    expect(panel).toContain("abortResearch");
  });

  it("检索计划：context_assemble 返回 execution_plan 并在面板展示", () => {
    expect(read("src/types/ai.ts")).toContain("execution_plan");
    expect(read("src-tauri/src/commands/ai_commands.rs")).toContain(
      "execution_plan",
    );
    expect(read("src/components/ai/UnifiedAssistantPanel.tsx")).toContain(
      "ExecutionPlanPreview",
    );
  });

  it("联网问答：助手传递 webAuthorized，状态由底栏指示器展示", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    expect(panel).toContain("webAuthorized: webSearch");
    expect(panel).not.toContain("联网已开");
    expect(read("src/components/layout/ConnectivityIndicators.tsx")).toContain(
      "联网",
    );
  });
});
