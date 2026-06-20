import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AgentTaskStatusPanel } from "@/components/ai/AgentTaskStatusPanel";
import type { AgentTaskDto } from "@/types/ipc";

const task: AgentTaskDto = {
  task_id: "task-delib",
  request_id: "req-delib",
  session_id: 1,
  kind: "complex",
  status: "completed",
  user_goal_summary: "完成行业研究",
  budget_policy: { mode: "complex" },
  created_at: "2026-06-20T00:00:00Z",
  updated_at: "2026-06-20T00:05:00Z",
  completed_at: "2026-06-20T00:05:00Z",
  error_code: null,
  error_message: null,
  deliberation_state: {
    request_id: "req-delib",
    session_id: 1,
    current_goal: "验证行业研究结论",
    plan_outline: ["拆分问题", "核验证据"],
    assumptions: ["token_budget=16000"],
    open_questions: ["缺少竞品收入数据"],
    evidence_gaps: ["缺少 2026 年一手收入数据"],
    verification_items: [
      {
        id: "evidence_accounted",
        description: "引用证据或明确说明无需外部证据",
        status: "failed",
      },
    ],
    status: "needs_attention",
  },
  verification_summary: {
    passed: false,
    items: [
      {
        id: "evidence_accounted",
        description: "引用证据或明确说明无需外部证据",
        status: "failed",
      },
    ],
  },
};

describe("AgentTaskStatusPanel deliberation state", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("keeps deliberation details folded but available without raw internals", async () => {
    await act(async () => {
      root.render(
        <AgentTaskStatusPanel
          task={task}
          steps={[]}
          events={[]}
          onAbort={vi.fn()}
          onOpenAudit={vi.fn()}
          onResume={vi.fn()}
        />,
      );
    });

    expect(document.body.textContent).toContain("过程详情");
    expect(document.body.textContent).not.toContain("缺少 2026 年一手收入数据");

    const button = Array.from(document.body.querySelectorAll("button")).find(
      (item) => item.textContent?.includes("过程详情"),
    );
    await act(async () => {
      button?.click();
    });

    expect(document.body.textContent).toContain("计划");
    expect(document.body.textContent).toContain("拆分问题");
    expect(document.body.textContent).toContain("证据缺口");
    expect(document.body.textContent).toContain("缺少 2026 年一手收入数据");
    expect(document.body.textContent).toContain("验证未通过");
    expect(document.body.textContent).toContain("查看审计");
    expect(document.body.textContent).not.toContain("checkpoint_json");
    expect(document.body.textContent).not.toContain("noteContent");
    expect(document.body.textContent).not.toContain("apiKey");
    expect(document.body.textContent).not.toContain("sha256");
    expect(document.body.textContent).not.toContain("preview=");
  });
});
