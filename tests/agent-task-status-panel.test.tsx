import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AgentTaskStatusPanel } from "@/components/ai/AgentTaskStatusPanel";
import type { AssistantArtifactDraft } from "@/types/assistant-artifact";
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
          onOpenArtifact={vi.fn()}
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
    const onOpenArtifact = vi.fn<(draft: AssistantArtifactDraft) => void>();

    await act(async () => {
      root.render(
        <AgentTaskStatusPanel
          task={baseTask}
          steps={steps}
          events={events}
          onAbort={onAbort}
          onOpenArtifact={onOpenArtifact}
          onResume={onResume}
        />,
      );
    });

    expect(document.body.textContent).toContain("继续");
    expect(document.body.textContent).toContain("中止");
    expect(document.body.textContent).toContain("过程详情");
    expect(document.body.textContent).not.toContain("调研合同风险");
    expect(document.body.textContent).not.toContain("找到两条证据");
    expect(document.body.textContent).not.toContain("checkpoint");
    expect(document.body.textContent).not.toContain("api_key");
    expect(document.body.textContent).not.toContain("raw_result");

    const summaryButton = Array.from(
      document.body.querySelectorAll("button"),
    ).find((button) => button.textContent?.includes("过程详情"));
    await act(async () => {
      summaryButton?.click();
    });

    expect(document.body.textContent).not.toContain("research");
    expect(document.body.textContent).not.toContain("找到两条证据");
    expect(document.body.textContent).not.toContain("引用 2");
    expect(document.body.textContent).not.toContain("等待写入授权");

    const continueButton = Array.from(
      document.body.querySelectorAll("button"),
    ).find((button) => button.textContent === "继续");
    const abortButton = Array.from(
      document.body.querySelectorAll("button"),
    ).find((button) => button.textContent === "中止");
    const processButton = Array.from(
      document.body.querySelectorAll("button"),
    ).find((button) => button.textContent?.includes("在工作区打开"));

    await act(async () => {
      continueButton?.click();
      abortButton?.click();
      processButton?.click();
    });

    expect(onResume).toHaveBeenCalledTimes(1);
    expect(onAbort).toHaveBeenCalledTimes(1);
    expect(onOpenArtifact).toHaveBeenCalledWith(
      expect.objectContaining({
        kind: "task_process",
        sourceRequestId: "req-1",
      }),
    );
    expect(document.body.textContent).not.toContain("查看审计");
  });

  it("does not expose a process artifact for ordinary completed tasks", async () => {
    const onOpenArtifact = vi.fn<(draft: AssistantArtifactDraft) => void>();
    const completedStep: AgentTaskStepDto = {
      ...steps[0]!,
      status: "completed",
      output_summary:
        "assistant task completed; no process artifact generated for ordinary completion",
    };

    await act(async () => {
      root.render(
        <AgentTaskStatusPanel
          task={{
            ...baseTask,
            status: "completed",
            completed_at: "2026-06-19T00:06:00Z",
          }}
          steps={[completedStep]}
          events={[]}
          onAbort={vi.fn()}
          onOpenArtifact={onOpenArtifact}
          onResume={vi.fn()}
        />,
      );
    });

    expect(document.body.textContent).not.toContain("过程详情");
    expect(document.body.textContent).not.toContain("已完成");
    expect(document.body.querySelector("button")).toBeNull();
    expect(onOpenArtifact).not.toHaveBeenCalled();
  });
});
