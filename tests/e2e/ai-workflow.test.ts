/**
 * 统一助手工作流验收（契约 + 路由语义）。
 */
import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { buildAssistantTaskPlan } from "@/lib/assistant-taskplan";

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

  it("选区改写：右键菜单与内联流式建议", () => {
    expect(intentForFlow("selection_rewrite")).toBe("writing");
    expect(read("src/App.tsx")).not.toContain("FloatingToolbar");
    expect(read("src/lib/editor-actions.ts")).not.toContain(
      "selection_toolbar",
    );
    expect(read("src/hooks/useEditorContextMenu.ts")).toContain(
      "filterEditorActions",
    );
    expect(read("src/components/editor/TipTapEditor.tsx")).toContain(
      "AiStreamExtension",
    );
  });

  it("引用检查：选区 + 检查语义进入 citation", () => {
    expect(intentForFlow("citation_check")).toBe("citation");
    expect(read("src/components/ai/AssistantTaskSurfaces.tsx")).toContain(
      'schema: "citation_report"',
    );
    expect(read("src/components/layout/ArtifactWorkspaceView.tsx")).toContain(
      "CitationCheckView",
    );
  });

  it("研究任务：对话摘要进入普通消息流，临时视图负责展开", () => {
    expect(intentForFlow("research_focus")).toBe("research");
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    const list = read("src/components/ai/AiMessageList.tsx");
    const tasks = read("src/components/ai/hooks/useAssistantTasks.ts");
    expect(list).toContain("AiMessageBubble");
    expect(list).not.toContain("ResearchResultMessage");
    expect(tasks).toContain("result.summary.trim()");
    expect(list).not.toContain("artifactLinks");
    expect(panel).toContain("abortResearch");
  });

  it("TaskPlan：小说续写即使包含分析研究词，也优先进入创作文字流", () => {
    const plan = buildAssistantTaskPlan({
      message:
        "根据以上文字续写第四章，先分析人物关系、研究剧情节奏并做一点综述，写得更火爆",
      hasSelection: false,
      notePath: "fiction/chapter-03.md",
      explicitScope: false,
      webAuthorized: true,
    });
    const list = read("src/components/ai/AiMessageList.tsx");

    expect(plan.intent).toBe("creative_write");
    expect(plan.modelSlot).toBe("writer");
    expect(plan.outputMode).toBe("markdown_message");
    expect(plan.artifactPlan).toEqual([]);
    expect(list).not.toContain("ResearchResultMessage");
    expect(list).not.toContain("证据矩阵");
  });

  it("TaskPlan：同会话明确联网研究真实资料时才进入研究产物路径", () => {
    const ordinaryMention = buildAssistantTaskPlan({
      message: "分析并综述一下这段故事为什么有效",
      hasSelection: false,
      notePath: "fiction/chapter-03.md",
      explicitScope: false,
      webAuthorized: false,
    });
    const plan = buildAssistantTaskPlan({
      message: "请联网研究真实资料，对比多来源证据，整理可信来源",
      hasSelection: false,
      notePath: null,
      explicitScope: true,
      webAuthorized: true,
    });

    expect(ordinaryMention.intent).not.toBe("research");
    expect(ordinaryMention.artifactPlan).toEqual([]);
    expect(plan.intent).toBe("research");
    expect(plan.webMode).toBe("brokered");
    expect(plan.outputMode).toBe("artifact_backed_message");
    expect(plan.artifactPlan).toEqual([
      expect.objectContaining({ kind: "evidence_sources" }),
    ]);
    expect(read("src/components/ai/AiMessageList.tsx")).not.toContain(
      "artifactLinks",
    );
    expect(read("src/components/ai/AssistantTaskSurfaces.tsx")).toContain(
      "AssistantArtifactTagStrip",
    );
  });

  it("TaskPlan：普通完成不生成无意义过程 tab", () => {
    const artifactTabs = read("src/lib/assistant-artifact-tabs.ts");
    const harnessTask = read("src-tauri/src/ai_harness/harness_task.rs");

    expect(artifactTabs).toContain("artifactPassesValueGate");
    expect(artifactTabs).not.toContain("fallbackDraftFromBody");
    expect(harnessTask).not.toContain(
      "assistant workflow output summarized by artifact metadata",
    );
  });

  it("选区写入：插入与替换工具执行前必须经过确认", () => {
    const rewritePlan = buildAssistantTaskPlan({
      message: "帮我改写这段文字",
      hasSelection: true,
      notePath: "notes/demo.md",
      explicitScope: false,
    });
    const toolConfirmDialog = read("src/components/ai/ToolConfirmDialog.tsx");
    const assistantTaskPlan = read("src/lib/assistant-taskplan.ts");
    const editorActions = read("src/lib/editor-actions.ts");

    expect(rewritePlan.intent).toBe("rewrite_selection");
    expect(rewritePlan.outputMode).toBe("confirmation_required");
    expect(editorActions).toContain('id: "send-to-ai"');
    expect(editorActions).toContain("requiresSelection: true");
    expect(toolConfirmDialog).toContain("insert_text_at_cursor");
    expect(toolConfirmDialog).toContain("replace_selection");
    expect(toolConfirmDialog).toContain("当前光标位置");
    expect(toolConfirmDialog).toContain("当前选区");
    expect(toolConfirmDialog).toContain("会直接修改当前笔记内容。");
    expect(assistantTaskPlan).toContain("confirmation_required");
  });

  it("检索计划：context_assemble 仍返回 execution_plan，面板自动继续不展示预览", () => {
    expect(read("src/types/ai.ts")).toContain("execution_plan");
    expect(read("src-tauri/src/commands/ai_commands.rs")).toContain(
      "execution_plan",
    );
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    expect(panel).not.toContain("ExecutionPlanPreview");
    expect(panel).toContain("assembleContextForChat");
    expect(panel).toContain("executeKnowledgeChat");
  });

  it("harness 现代化：tool_confirm 驱动 harness_resume 闭环", () => {
    const aiCommands = read("src-tauri/src/commands/ai_commands.rs");
    expect(aiCommands).toContain("resume_harness_after_tool_confirm");
    expect(aiCommands).toContain("append_rejected_tool_to_checkpoint");
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    expect(panel).toContain("toolConfirmIpc");
    expect(panel).toContain("已拒绝，正在生成替代回答");
  });

  it("harness 现代化：工具单源与统一 task 契约", () => {
    expect(read("src-tauri/src/ai_runtime/tool_dispatch.rs")).toContain(
      "DISPATCHABLE_TOOL_NAMES",
    );
    expect(read("src-tauri/src/ai_harness/harness_task.rs")).toContain(
      "run_harness_task",
    );
    expect(read("src/hooks/useAssistantRun.ts")).toContain("AssistantRunState");
    expect(read("src/lib/assistant-artifact-tabs.ts")).toContain(
      "artifactPassesValueGate",
    );
    expect(read("src/components/ai/hooks/useAssistantTasks.ts")).toContain(
      "buildArtifactDraftsFromTaskResult",
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

  it("产品文档：记录 TaskPlan harness 的对话流与临时 tab 规则", () => {
    const designSystem = read("docs/design-system.md");
    const roadmap = read("ROADMAP.md");
    const docsIndex = read("docs/README.md");

    expect(designSystem).toContain("Markdown-first");
    expect(designSystem).toContain("临时 tab 是高价值产物");
    expect(designSystem).toContain("过程 tab 只用于长任务");
    expect(designSystem).toContain("引用胶囊显示短摘要");
    expect(roadmap).toContain("TaskPlan");
    expect(roadmap).not.toContain("ResearchResultMessage + ResearchFocusView");
    expect(docsIndex).toContain(
      "2026-06-21-agent-harness-taskplan-blueprint-design.md",
    );
    expect(docsIndex).toContain(
      "2026-06-21-agent-harness-taskplan-blueprint.md",
    );
  });
});
