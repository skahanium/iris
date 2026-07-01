import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { buildAssistantTaskPlan } from "@/lib/assistant-taskplan";
import { isPlaceholderTitle } from "@/lib/path-sync";
import type {
  AssistantActionState,
  AssistantIntent,
  AssistantSurfaceState,
  AssistantTaskStatus,
} from "@/types/ai";

describe("note workflow helpers", () => {
  it("detects placeholder titles", () => {
    expect(isPlaceholderTitle("")).toBe(false);
    expect(isPlaceholderTitle("未命名文档")).toBe(true);
    expect(isPlaceholderTitle("新建文档")).toBe(true);
    expect(isPlaceholderTitle("untitled-1")).toBe(true);
    expect(isPlaceholderTitle("民法笔记")).toBe(false);
  });
});

describe("assistant per-turn TaskPlan dispatch", () => {
  it("keeps mixed-scene messages independent within one conversation", () => {
    const sequence = [
      {
        message: "这个概念是什么意思？",
        notePath: "/notes/topic.md",
        explicitScope: false,
        expected: "chat",
      },
      {
        message: "根据上文续写一段，剧情更诱人",
        notePath: "/notes/story.md",
        explicitScope: false,
        expected: "creative_write",
      },
      {
        message: "请联网研究这个主题的真实资料",
        notePath: null,
        explicitScope: true,
        expected: "research",
      },
      {
        message: "谢谢，简单说一下就行",
        notePath: null,
        explicitScope: false,
        expected: "chat",
      },
    ] as const;

    const intents = sequence.map(
      (turn) =>
        buildAssistantTaskPlan({
          message: turn.message,
          hasImage: false,
          hasSelection: false,
          notePath: turn.notePath,
          explicitScope: turn.explicitScope,
          contextReferences: [],
          webAuthorized: turn.message.includes("联网"),
        }).intent,
    );

    expect(intents).toEqual(sequence.map((turn) => turn.expected));
  });

  it("routes authorized freshness questions to brokered short answers without asking again", () => {
    const plan = buildAssistantTaskPlan({
      message:
        "2026年6月最新 Chatbot Arena / SWE-bench Live / LiveBench 榜单是什么？",
      hasImage: false,
      hasSelection: false,
      notePath: null,
      explicitScope: false,
      contextReferences: [],
      webAuthorized: true,
    });

    expect(plan.intent).toBe("chat");
    expect(plan.webMode).toBe("brokered");
    expect(plan.executionMode).toBe("direct_answer");
    expect(plan.artifactPlan).toEqual([]);
    expect(plan.evidenceNeed).toBe("fresh_web");
    expect(plan.requiresClarification).toBe(false);
    expect(plan.sourceHints).toContain("web:fresh_required");
  });

  it("does not treat the web toggle alone as a research command", () => {
    const plan = buildAssistantTaskPlan({
      message: "这个概念是什么意思？",
      hasImage: false,
      hasSelection: false,
      notePath: "/notes/topic.md",
      explicitScope: false,
      contextReferences: [],
      webAuthorized: true,
    });

    expect(plan.intent).toBe("chat");
    expect(plan.webMode).toBe("brokered");
    expect(plan.sourceHints).not.toContain("web:fresh_required");
  });

  it("uses TaskPlan as the primary send dispatcher contract", () => {
    const taskHook = readFileSync(
      "src/components/ai/hooks/useAssistantTasks.ts",
      "utf8",
    );

    expect(taskHook).toContain("buildAssistantTaskPlan({");
    expect(taskHook).toContain("switch (taskPlan.intent)");
    expect(taskHook).not.toContain("switch (actionState.intent)");
    expect(taskHook).toContain("taskPlan,");
    expect(taskHook).toContain('case "creative_write":');
    expect(taskHook).toContain('case "rewrite_selection":');
    expect(taskHook).toContain("await runWriting(rawMessage, taskPlan)");
    expect(taskHook).toContain('runKnowledgeChat(rawMessage, "chat"');
    expect(taskHook).toContain("setCurrentTaskPlanIntent(taskPlan.intent)");
  });

  it("does not sync a global legacy scene hint from panel effects", () => {
    const panelEffects = readFileSync(
      "src/components/ai/hooks/useAssistantPanelEffects.ts",
      "utf8",
    );

    expect(panelEffects).not.toContain("syncActiveLegacySceneHint");
  });

  it("keeps clarification turns text-only and clears stale task surfaces", () => {
    const taskHook = readFileSync(
      "src/components/ai/hooks/useAssistantTasks.ts",
      "utf8",
    );
    const clarificationBranch = taskHook.slice(
      taskHook.indexOf("if (taskPlan.requiresClarification)"),
      taskHook.indexOf("switch (taskPlan.intent)"),
    );

    expect(clarificationBranch).toContain("clearTaskSurfaces()");
    expect(clarificationBranch).toContain("setIntentDetection(null)");
    expect(clarificationBranch).toContain("setRunPlanSummary(null)");
    expect(clarificationBranch).toContain(
      "setPermissionPreflightSummary(null)",
    );
    expect(clarificationBranch).not.toContain("assistantExecute");
  });

  it("labels the status popover as current-turn state, not a fixed task scene", () => {
    const statusBadge = readFileSync(
      "src/components/ai/AgentStatusBadge.tsx",
      "utf8",
    );

    expect(statusBadge).toContain("本轮：");
    expect(statusBadge).toContain("taskPlanIntent");
    expect(statusBadge).toContain('case "creative_write"');
    expect(statusBadge).not.toContain("任务：");
  });

  it("labels citation checks separately from research synthesis", () => {
    const statusBadge = readFileSync(
      "src/components/ai/AgentStatusBadge.tsx",
      "utf8",
    );

    expect(statusBadge).toContain('case "citation_check"');
    expect(statusBadge).toContain('return "引用核查"');
    expect(statusBadge).toContain('case "research"');
    expect(statusBadge).toContain('return "研究综合"');
  });

  it("does not depend on the whole bubble selection object for selection quotes", () => {
    const panel = readFileSync(
      "src/components/ai/UnifiedAssistantPanel.impl.tsx",
      "utf8",
    );
    const hook = readFileSync(
      "src/components/ai/hooks/useSelectionQuoteReference.ts",
      "utf8",
    );

    expect(panel).not.toContain("}, [bubbleSelection, selectionQuote]);");
    expect(panel).toContain("useSelectionQuoteReference");
    expect(hook).toContain("quoteSelectionAsReference(");
  });
});

describe("assistant state types", () => {
  it("supports unified assistant intents and surface states", () => {
    const intents: AssistantIntent[] = [
      "chat",
      "knowledge",
      "writing",
      "citation",
      "organize",
      "research",
      "chapter",
      "document",
    ];
    const statuses: AssistantTaskStatus[] = [
      "idle",
      "running",
      "awaiting_confirmation",
      "completed",
      "error",
    ];
    const surfaceStates: AssistantSurfaceState[] = [
      "conversation",
      "inline_suggestion",
      "diff_review",
      "research_focus",
    ];
    const action: AssistantActionState = {
      intent: "knowledge",
      status: "idle",
      label: "知识查阅",
    };

    expect(intents).toHaveLength(8);
    expect(statuses).toContain("awaiting_confirmation");
    expect(surfaceStates).toContain("research_focus");
    expect(action.intent).toBe("knowledge");
  });
});
