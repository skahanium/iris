import { act, type Dispatch, type SetStateAction } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useAgentTaskStatus } from "@/components/ai/hooks/useAgentTaskStatus";
import { agentTaskEvents, agentTaskGet, agentTaskSteps } from "@/lib/ipc";
import type { AgentTaskDto } from "@/types/ipc";

vi.mock("@/lib/ipc", () => ({
  agentTaskAbort: vi.fn(),
  agentTaskEvents: vi.fn(),
  agentTaskGet: vi.fn(),
  agentTaskSteps: vi.fn(),
}));

const baseTask: AgentTaskDto = {
  task_id: "task-1",
  request_id: "req-1",
  session_id: 1,
  kind: "complex",
  status: "running",
  user_goal_summary: "diagnose standby crash",
  budget_policy: { mode: "complex" },
  created_at: "2026-07-06T00:00:00Z",
  updated_at: "2026-07-06T00:00:01Z",
  completed_at: null,
  error_code: null,
  error_message: null,
};

let setLastErrorMock: Dispatch<SetStateAction<string | null>>;
let setPausedTaskIdMock: Dispatch<SetStateAction<string | null>>;

function makeTask(status: AgentTaskDto["status"]): AgentTaskDto {
  return {
    ...baseTask,
    status,
    completed_at: status === "completed" ? "2026-07-06T00:01:00Z" : null,
    updated_at:
      status === "completed" ? "2026-07-06T00:01:00Z" : "2026-07-06T00:00:01Z",
  };
}

function StatusProbe({ taskId }: { taskId: string }) {
  const result = useAgentTaskStatus({
    taskId,
    setLastError: setLastErrorMock,
    setPausedTaskId: setPausedTaskIdMock,
  });

  return <output data-status={result.agentTask?.status ?? "none"} />;
}

describe("useAgentTaskStatus", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.useFakeTimers();
    setLastErrorMock = vi.fn();
    setPausedTaskIdMock = vi.fn();
    vi.mocked(agentTaskSteps).mockResolvedValue([]);
    vi.mocked(agentTaskEvents).mockResolvedValue([]);
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  function status() {
    return host.querySelector("output")?.getAttribute("data-status");
  }

  it("ignores stale overlapping poll responses after a newer terminal response", async () => {
    let resolveSecond!: (task: AgentTaskDto) => void;
    vi.mocked(agentTaskGet)
      .mockResolvedValueOnce(makeTask("running"))
      .mockReturnValueOnce(
        new Promise<AgentTaskDto>((resolve) => {
          resolveSecond = resolve;
        }),
      )
      .mockResolvedValueOnce(makeTask("completed"));

    await act(async () => {
      root.render(<StatusProbe taskId="task-1" />);
    });
    expect(status()).toBe("running");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(2500);
    });
    expect(agentTaskGet).toHaveBeenCalledTimes(2);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(2500);
    });
    expect(status()).toBe("completed");

    await act(async () => {
      resolveSecond(makeTask("running"));
      await Promise.resolve();
    });

    expect(status()).toBe("completed");
  });
});
