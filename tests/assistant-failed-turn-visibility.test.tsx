import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { AiMessageList, type ChatLine } from "@/components/ai/AiMessageList";
import { useAssistantConversationProjection } from "@/components/ai/hooks/useAssistantConversationProjection";
import { replayAssistantRunEvents } from "@/lib/assistant-run-events";
import type { AssistantRunEvent } from "@/types/ai";

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getTotalSize: () => count * 96,
    getVirtualItems: () =>
      Array.from({ length: count }, (_, index) => ({
        index,
        start: index * 96,
      })),
    measureElement: vi.fn(),
  }),
}));

/**
 * End-to-end glue check for the 2026-09-10 silent-failure report.
 *
 * The projection decides which ChatLine carries the failure marker, and
 * `AiMessageList` decides where it draws the footer. A data-level test on the
 * projection alone cannot catch a disagreement between the two — which is
 * exactly what shipped: the marker was placed on the assistant line while the
 * footer renders under the user line, so a failed Run showed nothing at all.
 */
function TranscriptProbe({
  run,
}: {
  run: ReturnType<typeof replayAssistantRunEvents>;
}) {
  const [messages, setMessages] = useState<ChatLine[]>([
    { role: "user", content: "最近新闻", runId: "run-1", turnId: "turn-1" },
    { role: "assistant", content: "", runId: "run-1", turnId: "turn-1" },
  ]);
  useAssistantConversationProjection({
    run,
    messages,
    setMessages: (updater) =>
      setMessages((previous) =>
        typeof updater === "function" ? updater(previous) : updater,
      ),
    setStreaming: () => undefined,
    setActivityHint: () => undefined,
    setError: () => undefined,
  });
  return <AiMessageList streaming={false} messages={messages} />;
}

function failedRun() {
  return replayAssistantRunEvents("run-1", [
    {
      runId: "run-1",
      seq: 1,
      stateVersion: 0,
      timestamp: "2026-09-10T16:09:16.000Z",
      type: "accepted",
      payload: {
        kind: "accepted",
        turnId: "turn-1",
        sessionKey: "session-1",
      },
    },
    {
      runId: "run-1",
      seq: 2,
      stateVersion: 1,
      timestamp: "2026-09-10T16:09:17.000Z",
      type: "failed",
      payload: {
        kind: "failed",
        code: "agent_run_incomplete_output",
        message: "回答未完整生成，请重试",
      },
    },
  ] satisfies AssistantRunEvent[]);
}

function runningRun() {
  return replayAssistantRunEvents("run-1", [
    {
      runId: "run-1",
      seq: 1,
      stateVersion: 0,
      timestamp: "2026-09-10T16:09:16.000Z",
      type: "accepted",
      payload: {
        kind: "accepted",
        turnId: "turn-1",
        sessionKey: "session-1",
      },
    },
    {
      runId: "run-1",
      seq: 2,
      stateVersion: 1,
      timestamp: "2026-09-10T16:09:17.000Z",
      type: "stage_changed",
      payload: { kind: "stage_changed", state: "running", stage: "正在处理" },
    },
  ] satisfies AssistantRunEvent[]);
}

describe("failed turn visibility", () => {
  let host: HTMLDivElement | null = null;
  let root: Root | null = null;

  afterEach(() => {
    act(() => root?.unmount());
    host?.remove();
    root = null;
    host = null;
  });

  function render(run: ReturnType<typeof replayAssistantRunEvents>) {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() => root?.render(<TranscriptProbe run={run} />));
    return host;
  }

  it("在转录区内渲染失败脚注，而不是让该轮消失", () => {
    const surface = render(failedRun());

    expect(surface.textContent).toContain("最近新闻");
    expect(surface.textContent).toContain(
      "本次请求未完成，未纳入后续对话上下文。",
    );
  });

  it("失败轮不发布未提交的候选正文", () => {
    const surface = render(failedRun());

    expect(surface.textContent).not.toContain("已经安全展示的前半段");
    // The empty answer slot is dropped rather than rendered as an empty bubble.
    expect(
      surface.querySelectorAll("[data-row-kind='citations']"),
    ).toHaveLength(0);
  });

  it("只有失败轮才带失败标记，进行中的轮次不受影响", () => {
    const surface = render(runningRun());

    expect(surface.textContent).toContain("最近新闻");
    expect(surface.textContent).not.toContain("本次请求未完成");
  });
});
