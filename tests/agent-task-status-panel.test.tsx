import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AgentTaskStatusPanel } from "@/components/ai/AgentTaskStatusPanel";
import type {
  AgentTaskDto,
  AgentTaskEventDto,
  AgentTaskStepDto,
} from "@/types/ipc";

const baseTask: AgentTaskDto = {
  task_id: "task-1",
  request_id: "req-1",
  session_id: 1,
  kind: "complex",
  status: "paused_budget",
  user_goal_summary: "调研合同风险",
  budget_policy: { mode: "complex" },
  created_at: "2026-06-19T00:00:00Z",
  updated_at: "2026-06-19T00:05:00Z",
  completed_at: null,
  error_code: null,
  error_message: null,
};

const steps: AgentTaskStepDto[] = [
  {
    id: 1,
    task_id: "task-1",
    step_seq: 1,
    kind: "research",
    status: "paused_budget",
    input_summary: "问题摘要",
    output_summary: "找到两条证据",
    evidence_packet_ids: ["pkt-a", "pkt-b"],
    created_at: "2026-06-19T00:01:00Z",
    updated_at: "2026-06-19T00:02:00Z",
  },
];

const events: AgentTaskEventDto[] = [
  {
    id: 1,
    task_id: "task-1",
    event_type: "permission_wait",
    message: "等待写入授权",
    created_at: "2026-06-19T00:03:00Z",
  },
];

describe("AgentTaskStatusPanel", () => {
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

  it("does not render task-system UI for lightweight chat tasks", async () => {
    await act(async () => {
      root.render(
        <AgentTaskStatusPanel
          task={{ ...baseTask, kind: "lightweight" }}
          steps={steps}
          events={events}
          onAbort={vi.fn()}
          onOpenAudit={vi.fn()}
          onResume={vi.fn()}
        />,
      );
    });

    expect(document.body.textContent).not.toContain("任务");
    expect(document.body.querySelector("button")).toBeNull();
  });

  it("shows safe complex task actions and progress summaries without raw checkpoint data", async () => {
    const onResume = vi.fn();
    const onAbort = vi.fn();
    const onOpenAudit = vi.fn();

    await act(async () => {
      root.render(
        <AgentTaskStatusPanel
          task={baseTask}
          steps={steps}
          events={events}
          onAbort={onAbort}
          onOpenAudit={onOpenAudit}
          onResume={onResume}
        />,
      );
    });

    expect(document.body.textContent).toContain("暂停");
    expect(document.body.textContent).toContain("调研合同风险");
    expect(document.body.textContent).toContain("继续");
    expect(document.body.textContent).toContain("中止");
    expect(document.body.textContent).toContain("查看进度摘要");
    expect(document.body.textContent).not.toContain("checkpoint");
    expect(document.body.textContent).not.toContain("api_key");
    expect(document.body.textContent).not.toContain("raw_result");

    const summaryButton = Array.from(
      document.body.querySelectorAll("button"),
    ).find((button) => button.textContent === "查看进度摘要");
    await act(async () => {
      summaryButton?.click();
    });

    expect(document.body.textContent).toContain("research");
    expect(document.body.textContent).toContain("找到两条证据");
    expect(document.body.textContent).toContain("引用 2");
    expect(document.body.textContent).toContain("等待写入授权");

    const continueButton = Array.from(
      document.body.querySelectorAll("button"),
    ).find((button) => button.textContent === "继续");
    const abortButton = Array.from(
      document.body.querySelectorAll("button"),
    ).find((button) => button.textContent === "中止");
    const auditButton = Array.from(
      document.body.querySelectorAll("button"),
    ).find((button) => button.textContent === "查看审计");

    await act(async () => {
      continueButton?.click();
      abortButton?.click();
      auditButton?.click();
    });

    expect(onResume).toHaveBeenCalledTimes(1);
    expect(onAbort).toHaveBeenCalledTimes(1);
    expect(onOpenAudit).toHaveBeenCalledTimes(1);
  });
});
