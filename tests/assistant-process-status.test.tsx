import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  AssistantProcessStatusBar,
  type ResearchProgressData,
} from "@/components/ai/AssistantProcessStatusBar";
import type { AgentTaskDto } from "@/types/ipc";

const runningTask: AgentTaskDto = {
  task_id: "task-running",
  request_id: "req-running",
  session_id: 1,
  kind: "complex",
  status: "running",
  user_goal_summary:
    "chars=36 sha256=a9fca530 preview=今天是2026.6.20,深度分析研究",
  budget_policy: {},
  created_at: "2026-06-20T00:00:00Z",
  updated_at: "2026-06-20T00:01:00Z",
  completed_at: null,
  error_code: null,
  error_message: null,
};

const progress: ResearchProgressData = {
  request_id: "req-progress",
  topic: "大模型行业研究",
  state: "running",
  current_round: 2,
  max_rounds: 4,
  queries_executed: [],
  new_evidence_count: 2,
  total_evidence_count: 6,
  tokens_used: 1200,
  token_budget: 16000,
  progress_pct: 0.5,
  round_terminated_early: false,
};

describe("AssistantProcessStatusBar", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-06-20T00:00:00Z"));
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.useRealTimers();
  });

  it("shows compact research progress without raw internals", async () => {
    await act(async () => {
      root.render(
        <AssistantProcessStatusBar
          agentTask={runningTask}
          activityHint="正在理解你的问题…"
          researchProgress={progress}
          researchRunning
          onAbort={vi.fn()}
        />,
      );
    });

    expect(document.body.textContent).toContain("正在研究");
    expect(document.body.textContent).toContain("第 2/4 轮");
    expect(document.body.textContent).toContain("已收集 6 条证据");
    expect(document.body.textContent).not.toContain("sha256");
    expect(document.body.textContent).not.toContain("preview=");
    expect(document.body.textContent).not.toContain("req-progress");
  });

  it("uses a visually distinct status strip instead of a message bubble", async () => {
    await act(async () => {
      root.render(
        <AssistantProcessStatusBar
          agentTask={runningTask}
          activityHint="正在理解你的问题…"
          researchProgress={null}
          researchRunning={false}
          onAbort={vi.fn()}
        />,
      );
    });

    const shell = document.querySelector(
      '[data-testid="assistant-process-status"]',
    );
    const strip = document.querySelector(
      '[data-testid="assistant-process-status-strip"]',
    );

    expect(shell?.className).toContain("pb-4");
    expect(strip?.className).toContain("border-l");
    expect(strip?.className).toContain("bg-transparent");
    expect(strip?.className).not.toContain("rounded-md");
    expect(strip?.className).not.toContain("bg-surface-inset");
  });

  it("changes long-running copy after eight seconds", async () => {
    await act(async () => {
      root.render(
        <AssistantProcessStatusBar
          agentTask={runningTask}
          activityHint="正在理解你的问题…"
          researchProgress={null}
          researchRunning={false}
          onAbort={vi.fn()}
        />,
      );
    });

    expect(document.body.textContent).toContain("正在理解");
    await act(async () => {
      vi.advanceTimersByTime(8_100);
    });

    expect(document.body.textContent).toContain("仍在处理");
    expect(document.body.textContent).toContain("中止");
  });

  it("does not duplicate the thinking bubble and composer stop button during ordinary streaming", async () => {
    await act(async () => {
      root.render(
        <AssistantProcessStatusBar
          agentTask={null}
          activityHint="正在生成回答"
          researchProgress={null}
          researchRunning={false}
          streaming
          onAbort={vi.fn()}
        />,
      );
    });

    expect(
      document.querySelector('[data-testid="assistant-process-status"]'),
    ).toBeNull();
    expect(document.body.textContent).not.toContain("正在分析");
    expect(document.body.textContent).not.toContain("中止");
  });

  it("hides when the task is completed", async () => {
    await act(async () => {
      root.render(
        <AssistantProcessStatusBar
          agentTask={{ ...runningTask, status: "completed" }}
          activityHint={null}
          researchProgress={null}
          researchRunning={false}
          onAbort={vi.fn()}
        />,
      );
    });

    expect(document.body.textContent).toBe("");
  });

  it("does not stay visible for a completed task with a stale activity hint", async () => {
    await act(async () => {
      root.render(
        <AssistantProcessStatusBar
          agentTask={{ ...runningTask, status: "completed" }}
          activityHint="正在理解你的问题…"
          researchProgress={null}
          researchRunning={false}
          streaming={false}
          hasError={false}
          onAbort={vi.fn()}
        />,
      );
    });

    expect(document.body.textContent).toBe("");
  });

  it("renders failed_safe as a terminal error state without spinner or abort", async () => {
    await act(async () => {
      root.render(
        <AssistantProcessStatusBar
          agentTask={{ ...runningTask, status: "failed_safe" }}
          activityHint="正在处理"
          researchProgress={null}
          researchRunning={false}
          hasError
          onAbort={vi.fn()}
        />,
      );
    });

    expect(document.body.textContent).toContain("处理遇到问题");
    expect(document.body.textContent).not.toContain("中止");
    expect(
      document.querySelector(
        '[data-testid="assistant-process-status-strip"] .animate-spin',
      ),
    ).toBeNull();

    await act(async () => {
      vi.advanceTimersByTime(8_100);
    });

    expect(document.body.textContent).not.toContain("仍在处理");
  });
});
