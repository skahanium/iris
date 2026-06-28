import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { AssistantProcessStatusBar } from "@/components/ai/AssistantProcessStatusBar";
import { AiComposer } from "@/components/ui/ai-composer";
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

  it("does not render the status bar for ordinary streaming with no agent task", () => {
    render({ streaming: true });

    expect(
      document.querySelector('[data-testid="assistant-process-status"]'),
    ).toBeNull();
    expect(document.body.textContent).not.toContain("中止");
  });

  it("uses the composer stop button as the ordinary streaming abort entry", () => {
    let clicked = false;
    act(() => {
      root.render(
        createElement(AiComposer, {
          value: "",
          onChange: () => {},
          onSubmit: () => {},
          streaming: true,
          disabled: true,
          onStop: () => {
            clicked = true;
          },
        }),
      );
    });
    const stopButton = document.querySelector('button[aria-label="停止生成"]');
    expect(stopButton).not.toBeNull();
    act(() => {
      stopButton?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
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
