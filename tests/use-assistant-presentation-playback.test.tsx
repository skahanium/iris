import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useAssistantPresentationPlayback } from "@/components/ai/hooks/useAssistantPresentationPlayback";
import type { ChatLine } from "@/components/ai/AiMessageList";
import type { AssistantPresentationState } from "@/lib/assistant-presentation";
import {
  createAssistantRunEventState,
  type AssistantRunEventState,
} from "@/lib/assistant-run-events";

let root: Root | null = null;
let host: HTMLDivElement | null = null;
let messages: ChatLine[] = [];

function Probe({
  presentation,
  run = null,
}: {
  presentation: AssistantPresentationState;
  run?: AssistantRunEventState | null;
}) {
  useAssistantPresentationPlayback({
    presentation,
    run,
    setMessages: (updater) => {
      messages = typeof updater === "function" ? updater(messages) : updater;
    },
  });
  return null;
}

function render(
  presentation: AssistantPresentationState,
  run: AssistantRunEventState | null = null,
) {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  act(() => root?.render(<Probe presentation={presentation} run={run} />));
}

function presentation(
  processItems: AssistantPresentationState["processItems"],
  answer = "",
  answerComplete = false,
): AssistantPresentationState {
  return {
    runId: "run-1",
    lastSeq: 5,
    resyncFromSeq: null,
    pendingEvents: [],
    processItems,
    answer,
    answerComplete,
  };
}

afterEach(() => {
  act(() => root?.unmount());
  host?.remove();
  root = null;
  host = null;
  messages = [];
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("useAssistantPresentationPlayback", () => {
  it("立即镜像过程项，不制造额外视觉延迟", () => {
    messages = [{ role: "assistant", content: "", runId: "run-1" }];
    render(
      presentation([
        {
          id: "stage:1",
          kind: "stage",
          label: "正在准备工具执行",
          status: "running",
          elapsedMs: 0,
        },
        {
          id: "tool:web-1",
          kind: "tool",
          label: "web_search",
          status: "completed",
          elapsedMs: 2,
          durationMs: 3,
        },
        {
          id: "tool:web-2",
          kind: "tool",
          label: "web_search",
          status: "completed",
          elapsedMs: 3,
          durationMs: 6700,
        },
      ]),
    );

    expect(messages[0]?.processItems).toHaveLength(3);
    expect(messages[0]?.processItems?.[2]?.durationMs).toBe(6700);
  });

  it("直接展示已收到的答案正文", () => {
    messages = [{ role: "assistant", content: "", runId: "run-1" }];
    render(presentation([], "你好👋", true));

    expect(messages[0]?.content).toBe("你好👋");
    expect(messages[0]?.presentationStreaming).toBe(false);
  });

  it("收到空答案后会清除待展示正文", () => {
    messages = [{ role: "assistant", content: "", runId: "run-1" }];
    render(presentation([], "候选正文", false));
    expect(messages[0]?.content).toBe("候选正文");

    act(() =>
      root?.render(<Probe presentation={presentation([], "", false)} />),
    );
    expect(messages[0]?.content).toBe("");
  });

  it("展示序号缺口或终态事实接管后，不会再覆盖可靠正文", () => {
    messages = [
      {
        role: "assistant",
        content: "可靠最终正文",
        runId: "run-1",
        presentationStreaming: false,
      },
    ];
    const completedRun: AssistantRunEventState = {
      ...createAssistantRunEventState("run-1"),
      state: "completed",
    };
    render(
      {
        ...presentation([], "候选正文", false),
        resyncFromSeq: 2,
      },
      completedRun,
    );
    expect(messages[0]?.content).toBe("可靠最终正文");
  });
});
