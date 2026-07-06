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
  user_goal_summary: "contract risk research",
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
    input_summary: "question summary",
    output_summary: "found two evidence items",
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
    message: "waiting for write permission",
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

    expect(host.querySelector('[data-testid="agent-task-panel"]')).toBeNull();
    expect(host.querySelector("button")).toBeNull();
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

    expect(
      host.querySelector('[data-testid="agent-task-panel"]'),
    ).not.toBeNull();
    expect(host.textContent).not.toContain("contract risk research");
    expect(host.textContent).not.toContain("found two evidence items");
    expect(host.textContent).not.toContain("checkpoint");
    expect(host.textContent).not.toContain("api_key");
    expect(host.textContent).not.toContain("raw_result");

    const buttons = Array.from(host.querySelectorAll("button"));
    expect(buttons.length).toBe(3);

    await act(async () => {
      buttons[0]?.click();
      buttons[1]?.click();
      buttons[2]?.click();
    });

    expect(onOpenArtifact).toHaveBeenCalledWith(
      expect.objectContaining({
        kind: "task_process",
        sourceRequestId: "req-1",
      }),
    );
    expect(onResume).toHaveBeenCalledTimes(1);
    expect(onAbort).toHaveBeenCalledTimes(1);
    expect(host.textContent).not.toContain("research");
    expect(host.textContent).not.toContain("waiting for write permission");
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

    expect(host.querySelector('[data-testid="agent-task-panel"]')).toBeNull();
    expect(host.querySelector("button")).toBeNull();
    expect(onOpenArtifact).not.toHaveBeenCalled();
  });

  it("ignores malformed task detail fields instead of crashing the render boundary", async () => {
    const malformedTask = {
      ...baseTask,
      status: "stalled",
      verification_summary: {
        items: null,
      },
    } as unknown as AgentTaskDto;
    const malformedEvents = [
      {
        ...events[0]!,
        event_type: null,
      },
    ] as unknown as AgentTaskEventDto[];

    await act(async () => {
      root.render(
        <AgentTaskStatusPanel
          task={malformedTask}
          steps={steps}
          events={malformedEvents}
          onAbort={vi.fn()}
          onOpenArtifact={vi.fn()}
          onResume={vi.fn()}
        />,
      );
    });

    expect(host.textContent).not.toContain("undefined");
  });
});
