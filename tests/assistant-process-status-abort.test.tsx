import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { AssistantProcessStatusBar } from "@/components/ai/AssistantProcessStatusBar";
import type { AgentTaskDto } from "@/types/ipc";

describe("AssistantProcessStatusBar abort visibility", () => {
  let root: Root;
  let container: HTMLDivElement;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
  });

  function render(props: {
    streaming?: boolean;
    researchRunning?: boolean;
    agentTask?: AgentTaskDto | null;
    activityHint?: string | null;
  }) {
    act(() => {
      root.render(
        createElement(AssistantProcessStatusBar, {
          activityHint: props.activityHint ?? "正在连接模型并处理工具调用…",
          agentTask: props.agentTask ?? null,
          researchProgress: null,
          researchRunning: props.researchRunning ?? false,
          streaming: props.streaming,
          onAbort: () => {},
        }),
      );
    });
  }

  it("renders the status bar and abort button when streaming with no agent task", () => {
    // Symptom: chat hangs, agentTaskId is null, researchRunning false.
    // Before fix: `active` and `canAbort` ignored `streaming`, so the bar
    // vanished entirely and the user had no abort entry point.
    render({ streaming: true });

    expect(
      document.querySelector('[data-testid="assistant-process-status"]'),
    ).not.toBeNull();
    const abortBtn = document.querySelector("button");
    expect(abortBtn).not.toBeNull();
    expect(abortBtn?.textContent).toContain("中止");
  });

  it("clicking the abort button invokes onAbort while streaming", () => {
    let clicked = false;
    act(() => {
      root.render(
        createElement(AssistantProcessStatusBar, {
          activityHint: "正在连接模型并处理工具调用…",
          agentTask: null,
          researchProgress: null,
          researchRunning: false,
          streaming: true,
          onAbort: () => {
            clicked = true;
          },
        }),
      );
    });
    const abortBtn = document.querySelector("button");
    expect(abortBtn).not.toBeNull();
    act(() => {
      abortBtn?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(clicked).toBe(true);
  });

  it("does not render the bar when idle (no streaming, no task, no research)", () => {
    render({ streaming: false });
    expect(
      document.querySelector('[data-testid="assistant-process-status"]'),
    ).toBeNull();
  });
});
