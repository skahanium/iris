import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AgentTaskStatusPanel } from "@/components/ai/AgentTaskStatusPanel";
import type { AssistantArtifactDraft } from "@/types/assistant-artifact";
import type { AgentTaskDto, AgentTaskEventDto } from "@/types/ipc";

const baseTask: AgentTaskDto = {
  task_id: "task-long",
  request_id: "req-long",
  session_id: 1,
  kind: "complex",
  status: "paused_budget",
  user_goal_summary: "long running task",
  budget_policy: { mode: "complex" },
  created_at: "2026-07-06T00:00:00Z",
  updated_at: "2026-07-06T00:01:00Z",
  completed_at: null,
  error_code: null,
  error_message: null,
};

describe("AgentTaskStatusPanel payload budget", () => {
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

  it("opens a bounded process artifact instead of retaining full event payloads", async () => {
    const hugeMessage = `raw-event-${"X".repeat(180_000)}-tail`;
    const onOpenArtifact = vi.fn<(draft: AssistantArtifactDraft) => void>();
    const events = [
      {
        id: 1,
        task_id: "task-long",
        event_type: "permission_wait",
        message: hugeMessage,
        created_at: "2026-07-06T00:00:30Z",
      },
    ] satisfies AgentTaskEventDto[];

    await act(async () => {
      root.render(
        <AgentTaskStatusPanel
          task={baseTask}
          steps={[]}
          events={events}
          onAbort={vi.fn()}
          onOpenArtifact={onOpenArtifact}
          onResume={vi.fn()}
        />,
      );
    });

    const openButton = host.querySelector("button");
    expect(openButton).not.toBeNull();

    await act(async () => {
      openButton?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    const artifact = onOpenArtifact.mock.calls[0]?.[0];
    expect(artifact).toBeTruthy();
    const payload = JSON.stringify(artifact?.payload);
    expect(payload.length).toBeLessThan(35_000);
    expect(payload).not.toContain("X".repeat(50_000));
    expect(payload).toContain("contentRef");
  });
});
