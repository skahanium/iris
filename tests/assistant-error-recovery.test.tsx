import { act, useCallback, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AssistantErrorRecovery } from "@/components/ai/AssistantErrorRecovery";

function RecoverableTaskHarness() {
  const [pausedTaskId, setPausedTaskId] = useState<string | null>(
    "orphan-task",
  );
  const resetAssistantSessionState = useCallback(() => {
    setPausedTaskId(null);
  }, []);

  return (
    <>
      <AssistantErrorRecovery
        disabled={false}
        harnessRequestId={null}
        lastError={null}
        pausedTaskId={pausedTaskId}
        onResume={() => undefined}
      />
      <button type="button" onClick={resetAssistantSessionState}>
        删除当前会话
      </button>
    </>
  );
}

describe("AssistantErrorRecovery", () => {
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

  it("does not offer resume when a paused task belongs to another vault", async () => {
    const onResume = vi.fn();
    await act(async () => {
      root.render(
        <AssistantErrorRecovery
          disabled={false}
          harnessRequestId={null}
          lastError="RESUME_PREFLIGHT_FAILED: agent task resume preflight failed: vault scope changed"
          pausedTaskId="task-1"
          onResume={onResume}
        />,
      );
    });

    expect(document.body.textContent).toContain("当前库已变更");
    expect(document.body.textContent).not.toContain("继续任务");
    expect(document.body.querySelector("button")).toBeNull();
  });

  it("removes the paused task resume affordance after current session deletion resets state", async () => {
    await act(async () => {
      root.render(<RecoverableTaskHarness />);
    });

    expect(document.body.textContent).toContain("继续任务");

    const deleteButton = Array.from(
      document.body.querySelectorAll("button"),
    ).find((button) => button.textContent === "删除当前会话");
    await act(async () => {
      deleteButton?.click();
    });

    expect(document.body.textContent).not.toContain("继续任务");
  });
});
