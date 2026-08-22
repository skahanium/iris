import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  type AssistantAnswerReveal,
  useAssistantAnswerReveal,
} from "@/components/ai/hooks/useAssistantAnswerReveal";
import { useAssistantConversationProjection } from "@/components/ai/hooks/useAssistantConversationProjection";
import { restoreChatLineContent } from "@/lib/ai-payload-store";
import type { ChatLine } from "@/components/ai/AiMessageList";
import {
  ANSWER_COMPLETE_PROCESS_LABEL,
  type AssistantPresentationState,
} from "@/lib/assistant-presentation";
import { replayAssistantRunEvents } from "@/lib/assistant-run-events";
import type { AssistantRunEvent } from "@/types/ai";

let root: Root | null = null;
let host: HTMLDivElement | null = null;
let messages: ChatLine[] = [];
let streaming = false;

function Probe({
  run,
  presentation,
  presentationReveal,
}: {
  run: ReturnType<typeof replayAssistantRunEvents>;
  presentation?: AssistantPresentationState | null;
  presentationReveal?: AssistantAnswerReveal;
}) {
  useAssistantConversationProjection({
    run,
    presentation,
    presentationReveal:
      presentationReveal ??
      (presentation
        ? {
            runId: presentation.runId,
            answer: presentation.answer,
            revealing: false,
          }
        : undefined),
    messages,
    setMessages: (updater) => {
      messages = typeof updater === "function" ? updater(messages) : updater;
    },
    setStreaming: (next) => {
      streaming = next;
    },
    setActivityHint: () => undefined,
    setError: () => undefined,
  });
  return null;
}

function RevealProjectionProbe({
  run,
  presentation,
}: {
  run: ReturnType<typeof replayAssistantRunEvents>;
  presentation: AssistantPresentationState;
}) {
  const reveal = useAssistantAnswerReveal(presentation);
  useAssistantConversationProjection({
    run,
    presentation,
    presentationReveal: reveal,
    messages,
    setMessages: (updater) => {
      messages = typeof updater === "function" ? updater(messages) : updater;
    },
    setStreaming: (next) => {
      streaming = next;
    },
    setActivityHint: () => undefined,
    setError: () => undefined,
  });
  return null;
}

afterEach(() => {
  act(() => root?.unmount());
  host?.remove();
  root = null;
  host = null;
  messages = [];
  streaming = false;
});

describe("useAssistantConversationProjection", () => {
  it("连续 presentation delta 必须累积显示，而不是停留在首段", () => {
    messages = [
      { role: "user", content: "你好", runId: "run-1", turnId: "turn-1" },
      { role: "assistant", content: "", runId: "run-1", turnId: "turn-1" },
    ];
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    const running = replayAssistantRunEvents("run-1", [
      {
        runId: "run-1",
        seq: 1,
        stateVersion: 0,
        timestamp: "2026-08-03T00:00:00.000Z",
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
        timestamp: "2026-08-03T00:00:01.000Z",
        type: "stage_changed",
        payload: {
          kind: "stage_changed",
          state: "running",
          stage: "正在生成答复",
        },
      },
    ] satisfies AssistantRunEvent[]);

    act(() =>
      root?.render(
        <Probe
          run={running}
          presentation={{
            runId: "run-1",
            lastSeq: 2,
            resyncFromSeq: null,
            pendingEvents: [],
            processItems: [],
            answer: "第一段",
            answerComplete: false,
          }}
        />,
      ),
    );
    expect(messages[1]?.content).toBe("第一段");

    act(() =>
      root?.render(
        <Probe
          run={running}
          presentation={{
            runId: "run-1",
            lastSeq: 3,
            resyncFromSeq: null,
            pendingEvents: [],
            processItems: [],
            answer: "第一段第二段",
            answerComplete: false,
          }}
        />,
      ),
    );
    expect(messages[1]?.content).toBe("第一段第二段");
  });

  it("answerComplete 先到而 durable completed 丢失时仍结束 streaming", () => {
    messages = [
      { role: "user", content: "你好", runId: "run-1", turnId: "turn-1" },
      {
        role: "assistant",
        content: "完整答复",
        runId: "run-1",
        turnId: "turn-1",
      },
    ];
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    const running = replayAssistantRunEvents("run-1", [
      {
        runId: "run-1",
        seq: 1,
        stateVersion: 0,
        timestamp: "2026-07-22T08:00:00.000Z",
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
        timestamp: "2026-07-22T08:00:01.000Z",
        type: "stage_changed",
        payload: {
          kind: "stage_changed",
          state: "running",
          stage: "正在生成答复",
        },
      },
    ] satisfies AssistantRunEvent[]);

    act(() =>
      root?.render(
        <Probe
          presentation={{
            runId: "run-1",
            lastSeq: 2,
            resyncFromSeq: null,
            pendingEvents: [],
            processItems: [],
            answer: "完整答复",
            answerComplete: false,
          }}
          run={running}
        />,
      ),
    );
    expect(streaming).toBe(true);

    act(() =>
      root?.render(
        <Probe
          presentation={{
            runId: "run-1",
            lastSeq: 3,
            resyncFromSeq: null,
            pendingEvents: [],
            processItems: [],
            answer: "完整答复",
            answerComplete: true,
          }}
          run={running}
        />,
      ),
    );
    expect(streaming).toBe(false);
  });

  it("终态遇到展示序号缺口时以可靠事实正文收敛，不遗留局部答案", () => {
    messages = [
      { role: "user", content: "你好", runId: "run-1", turnId: "turn-1" },
      {
        role: "assistant",
        content: "局部",
        runId: "run-1",
        turnId: "turn-1",
      },
    ];
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() =>
      root?.render(
        <Probe
          presentation={{
            runId: "run-1",
            lastSeq: 4,
            resyncFromSeq: 5,
            pendingEvents: [],
            processItems: [],
            answer: "局部",
            answerComplete: false,
          }}
          run={replayAssistantRunEvents("run-1", [
            {
              runId: "run-1",
              seq: 1,
              stateVersion: 0,
              timestamp: "2026-07-22T08:00:00.000Z",
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
              timestamp: "2026-07-22T08:00:01.000Z",
              type: "stage_changed",
              payload: {
                kind: "stage_changed",
                state: "preparing",
                stage: "正在准备",
              },
            },
            {
              runId: "run-1",
              seq: 3,
              stateVersion: 2,
              timestamp: "2026-07-22T08:00:02.000Z",
              type: "stage_changed",
              payload: {
                kind: "stage_changed",
                state: "running",
                stage: "正在生成答复",
              },
            },
            {
              runId: "run-1",
              seq: 4,
              stateVersion: 2,
              timestamp: "2026-07-22T08:00:03.000Z",
              type: "content_delta",
              payload: { kind: "content_delta", delta: "可靠最终正文" },
            },
            {
              runId: "run-1",
              seq: 5,
              stateVersion: 3,
              timestamp: "2026-07-22T08:00:04.000Z",
              type: "completed",
              payload: { kind: "completed", messageId: "message-1" },
            },
          ] satisfies AssistantRunEvent[])}
        />,
      ),
    );

    expect(messages[1]?.content).toBe("可靠最终正文");
  });

  it("取消时若直播正文尚未 complete，仍保留半成品气泡并提示可继续", () => {
    messages = [
      { role: "user", content: "写一篇长文", runId: "run-1", turnId: "turn-1" },
      {
        role: "assistant",
        content: "这是已经流式露出的半成品正文",
        runId: "run-1",
        turnId: "turn-1",
        processItems: [
          {
            id: "tool:web-1",
            kind: "tool",
            label: "联网搜索",
            status: "running",
            createdAt: 1,
          },
        ],
      },
    ];
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() =>
      root?.render(
        <Probe
          presentation={{
            runId: "run-1",
            lastSeq: 3,
            resyncFromSeq: null,
            pendingEvents: [],
            processItems: [
              {
                id: "tool:web-1",
                kind: "tool",
                label: "联网搜索",
                status: "running",
                elapsedMs: 1,
              },
            ],
            answer: "这是已经流式露出的半成品正文",
            answerComplete: false,
          }}
          run={replayAssistantRunEvents("run-1", [
            {
              runId: "run-1",
              seq: 1,
              stateVersion: 0,
              timestamp: "2026-07-22T08:00:00.000Z",
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
              timestamp: "2026-07-22T08:00:01.000Z",
              type: "stage_changed",
              payload: {
                kind: "stage_changed",
                state: "preparing",
                stage: "正在准备",
              },
            },
            {
              runId: "run-1",
              seq: 3,
              stateVersion: 2,
              timestamp: "2026-07-22T08:00:02.000Z",
              type: "stage_changed",
              payload: {
                kind: "stage_changed",
                state: "running",
                stage: "正在生成答复",
              },
            },
            {
              runId: "run-1",
              seq: 4,
              stateVersion: 3,
              timestamp: "2026-07-22T08:00:03.000Z",
              type: "cancelled",
              payload: { kind: "cancelled", reason: "user_cancelled" },
            },
          ] satisfies AssistantRunEvent[])}
        />,
      ),
    );

    expect(messages[1]?.role).toBe("assistant");
    expect(messages[1]?.content).toBe("这是已经流式露出的半成品正文");
    expect(
      messages.some((message) => message.content.includes("发送继续")),
    ).toBe(true);
    expect(
      messages[1]?.processItems?.some((item) => item.label === "答复完毕"),
    ).toBe(false);
  });

  it("失败终态保留已露出的正文，但过程回落为失败而非答复完毕", () => {
    messages = [
      { role: "user", content: "最近新闻", runId: "run-1", turnId: "turn-1" },
      {
        role: "assistant",
        content: "已经安全展示的前半段。",
        runId: "run-1",
        turnId: "turn-1",
        processItems: [
          {
            id: "stage:generating",
            kind: "stage",
            label: "正在生成答复",
            status: "running",
            createdAt: 1,
          },
        ],
      },
    ];
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() =>
      root?.render(
        <Probe
          presentation={{
            runId: "run-1",
            lastSeq: 3,
            resyncFromSeq: null,
            pendingEvents: [],
            processItems: [],
            answer: "已经安全展示的前半段。",
            answerComplete: false,
          }}
          run={replayAssistantRunEvents("run-1", [
            {
              runId: "run-1",
              seq: 1,
              stateVersion: 0,
              timestamp: "2026-08-06T00:00:00.000Z",
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
              timestamp: "2026-08-06T00:00:01.000Z",
              type: "stage_changed",
              payload: {
                kind: "stage_changed",
                state: "running",
                stage: "正在生成答复",
              },
            },
            {
              runId: "run-1",
              seq: 3,
              stateVersion: 2,
              timestamp: "2026-08-06T00:00:02.000Z",
              type: "failed",
              payload: {
                kind: "failed",
                code: "agent_run_incomplete_output",
                message: "回答未完整生成，请重试",
              },
            },
          ] satisfies AssistantRunEvent[])}
        />,
      ),
    );

    expect(messages[1]?.content).toBe("已经安全展示的前半段。");
    expect(messages[1]?.presentationStreaming).toBe(false);
    expect(
      messages[1]?.processItems?.some((item) => item.label === "答复完毕"),
    ).toBe(false);
  });

  it("直播中展示序号缺口时保留已露出正文，不用空耐久正文覆盖", () => {
    messages = [
      { role: "user", content: "你好", runId: "run-1", turnId: "turn-1" },
      {
        role: "assistant",
        content: "已经露出的局部",
        runId: "run-1",
        turnId: "turn-1",
      },
    ];
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() =>
      root?.render(
        <Probe
          presentation={{
            runId: "run-1",
            lastSeq: 1,
            resyncFromSeq: 2,
            pendingEvents: [],
            processItems: [],
            answer: "已经露出的局部",
            answerComplete: false,
          }}
          run={replayAssistantRunEvents("run-1", [
            {
              runId: "run-1",
              seq: 1,
              stateVersion: 0,
              timestamp: "2026-07-22T08:00:00.000Z",
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
              timestamp: "2026-07-22T08:00:01.000Z",
              type: "stage_changed",
              payload: {
                kind: "stage_changed",
                state: "running",
                stage: "正在生成答复",
              },
            },
          ] satisfies AssistantRunEvent[])}
        />,
      ),
    );

    expect(messages[1]?.content).toBe("已经露出的局部");
  });

  it("adds a durable content delta only to the active assistant placeholder", () => {
    messages = [
      { role: "user", content: "你好", runId: "run-1", turnId: "turn-1" },
      { role: "assistant", content: "", runId: "run-1", turnId: "turn-1" },
    ];
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() =>
      root?.render(
        <Probe
          run={replayAssistantRunEvents("run-1", [
            {
              runId: "run-1",
              seq: 1,
              stateVersion: 0,
              timestamp: "2026-07-13T12:00:00.000Z",
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
              timestamp: "2026-07-13T12:00:01.000Z",
              type: "stage_changed",
              payload: {
                kind: "stage_changed",
                state: "preparing",
                stage: "正在准备",
              },
            },
            {
              runId: "run-1",
              seq: 3,
              stateVersion: 2,
              timestamp: "2026-07-13T12:00:02.000Z",
              type: "stage_changed",
              payload: {
                kind: "stage_changed",
                state: "running",
                stage: "正在生成答复",
              },
            },
            {
              runId: "run-1",
              seq: 4,
              stateVersion: 2,
              timestamp: "2026-07-13T12:00:03.000Z",
              type: "content_delta",
              payload: { kind: "content_delta", delta: "世界" },
            },
          ] satisfies AssistantRunEvent[])}
        />,
      ),
    );

    expect(messages).toMatchObject([
      { role: "user", content: "你好", runId: "run-1", turnId: "turn-1" },
      {
        role: "assistant",
        content: "世界",
        runId: "run-1",
        turnId: "turn-1",
        processItems: [{ id: "stage:3", label: "正在生成答复" }],
      },
    ]);
  });

  it("projects safe Run process items onto the bound assistant message", () => {
    messages = [
      { role: "user", content: "核验资料", runId: "run-1", turnId: "turn-1" },
      { role: "assistant", content: "", runId: "run-1", turnId: "turn-1" },
    ];
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() =>
      root?.render(
        <Probe
          run={replayAssistantRunEvents("run-1", [
            {
              runId: "run-1",
              seq: 1,
              stateVersion: 0,
              timestamp: "2026-07-22T08:00:00.000Z",
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
              timestamp: "2026-07-22T08:00:01.000Z",
              type: "reasoning_summary",
              payload: {
                kind: "reasoning_summary",
                summaryId: "summary-1",
                text: "先核验资料，再组织答案。",
              },
            },
            {
              runId: "run-1",
              seq: 3,
              stateVersion: 1,
              timestamp: "2026-07-22T08:00:02.000Z",
              type: "tool_started",
              payload: {
                kind: "tool_started",
                capability: "web_search",
                toolCallId: "tool-1",
              },
            },
          ] satisfies AssistantRunEvent[])}
        />,
      ),
    );

    expect(messages[1]).toMatchObject({
      content: "",
      processItems: [
        {
          id: "reasoning:summary-1",
          kind: "reasoning_summary",
          label: "先核验资料，再组织答案。",
        },
        {
          id: "tool:tool-1",
          kind: "tool",
          label: "联网搜索",
          status: "running",
        },
      ],
    });
  });

  it("updates the assistant slot bound to the Run even when it is not last", () => {
    messages = [
      { role: "user", content: "第一问", runId: "run-1", turnId: "turn-1" },
      { role: "assistant", content: "", runId: "run-1", turnId: "turn-1" },
      { role: "user", content: "第二问", runId: "run-2", turnId: "turn-2" },
      { role: "assistant", content: "", runId: "run-2", turnId: "turn-2" },
    ];
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() =>
      root?.render(
        <Probe
          run={replayAssistantRunEvents("run-1", [
            {
              runId: "run-1",
              seq: 1,
              stateVersion: 0,
              timestamp: "2026-07-13T12:00:00.000Z",
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
              stateVersion: 0,
              timestamp: "2026-07-13T12:00:01.000Z",
              type: "content_delta",
              payload: { kind: "content_delta", delta: "第一答" },
            },
          ] satisfies AssistantRunEvent[])}
        />,
      ),
    );

    expect(messages[1]?.content).toBe("第一答");
    expect(messages[3]?.content).toBe("");
  });

  it("removes only the empty assistant slot for the failed Run", () => {
    messages = [
      { role: "user", content: "第一问", runId: "run-1", turnId: "turn-1" },
      { role: "assistant", content: "", runId: "run-1", turnId: "turn-1" },
      { role: "user", content: "第二问", runId: "run-2", turnId: "turn-2" },
      { role: "assistant", content: "", runId: "run-2", turnId: "turn-2" },
    ];
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() =>
      root?.render(
        <Probe
          run={replayAssistantRunEvents("run-1", [
            {
              runId: "run-1",
              seq: 1,
              stateVersion: 0,
              timestamp: "2026-07-13T12:00:00.000Z",
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
              timestamp: "2026-07-13T12:00:01.000Z",
              type: "stage_changed",
              payload: {
                kind: "stage_changed",
                state: "preparing",
                stage: "正在准备",
              },
            },
            {
              runId: "run-1",
              seq: 3,
              stateVersion: 2,
              timestamp: "2026-07-13T12:00:02.000Z",
              type: "stage_changed",
              payload: {
                kind: "stage_changed",
                state: "running",
                stage: "正在运行",
              },
            },
            {
              runId: "run-1",
              seq: 4,
              stateVersion: 3,
              timestamp: "2026-07-13T12:00:03.000Z",
              type: "failed",
              payload: {
                kind: "failed",
                code: "agent_run_empty_output",
                message: "未生成可用回答",
              },
            },
          ] satisfies AssistantRunEvent[])}
        />,
      ),
    );

    expect(messages.map((message) => message.runId)).toEqual([
      "run-1",
      "run-2",
      "run-2",
    ]);
  });

  it("presentation 冻结 processItems 且 run 已 completed 时末项收敛为答复完毕", () => {
    messages = [
      { role: "user", content: "你好", runId: "run-1", turnId: "turn-1" },
      {
        role: "assistant",
        content: "完整答复",
        runId: "run-1",
        turnId: "turn-1",
        processItems: [
          {
            id: "stage:3",
            kind: "stage",
            label: "正在生成答复",
            status: "completed",
            createdAt: 3,
          },
        ],
      },
    ];
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    const run = replayAssistantRunEvents("run-1", [
      {
        runId: "run-1",
        seq: 1,
        stateVersion: 0,
        timestamp: "2026-07-22T08:00:00.000Z",
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
        timestamp: "2026-07-22T08:00:01.000Z",
        type: "stage_changed",
        payload: {
          kind: "stage_changed",
          state: "preparing",
          stage: "正在准备",
        },
      },
      {
        runId: "run-1",
        seq: 3,
        stateVersion: 2,
        timestamp: "2026-07-22T08:00:02.000Z",
        type: "stage_changed",
        payload: {
          kind: "stage_changed",
          state: "running",
          stage: "正在生成答复",
        },
      },
      {
        runId: "run-1",
        seq: 4,
        stateVersion: 2,
        timestamp: "2026-07-22T08:00:03.000Z",
        type: "content_delta",
        payload: { kind: "content_delta", delta: "完整答复" },
      },
      {
        runId: "run-1",
        seq: 5,
        stateVersion: 3,
        timestamp: "2026-07-22T08:00:04.000Z",
        type: "completed",
        payload: { kind: "completed", messageId: "message-1" },
      },
    ] satisfies AssistantRunEvent[]);
    expect(run.state).toBe("completed");

    act(() =>
      root?.render(
        <Probe
          presentation={{
            runId: "run-1",
            lastSeq: 5,
            resyncFromSeq: null,
            pendingEvents: [],
            processItems: [
              {
                id: "stage:3",
                kind: "stage",
                label: "正在生成答复",
                status: "completed",
                elapsedMs: 1,
              },
            ],
            answer: "完整答复",
            answerComplete: true,
          }}
          run={run}
        />,
      ),
    );

    expect(messages[1]?.processItems?.at(-1)?.label).toBe(
      ANSWER_COMPLETE_PROCESS_LABEL,
    );
  });

  it("ignores a late Run event when no transcript slot is bound to it", () => {
    messages = [
      { role: "user", content: "当前问题", runId: "run-2", turnId: "turn-2" },
      {
        role: "assistant",
        content: "当前回答",
        runId: "run-2",
        turnId: "turn-2",
      },
    ];
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() =>
      root?.render(
        <Probe
          run={replayAssistantRunEvents("run-late", [
            {
              runId: "run-late",
              seq: 1,
              stateVersion: 0,
              timestamp: "2026-07-13T12:00:00.000Z",
              type: "accepted",
              payload: {
                kind: "accepted",
                turnId: "turn-old",
                sessionKey: "session-1",
              },
            },
            {
              runId: "run-late",
              seq: 2,
              stateVersion: 0,
              timestamp: "2026-07-13T12:00:01.000Z",
              type: "content_delta",
              payload: { kind: "content_delta", delta: "迟到回答" },
            },
          ] satisfies AssistantRunEvent[])}
        />,
      ),
    );

    expect(messages[1]?.content).toBe("当前回答");
  });

  it("uses the smoothed presentation answer when reveal is active", () => {
    messages = [
      { role: "user", content: "你好", runId: "run-reveal", turnId: "turn-1" },
      {
        role: "assistant",
        content: "",
        runId: "run-reveal",
        turnId: "turn-1",
      },
    ];
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    const running = replayAssistantRunEvents("run-reveal", [
      {
        runId: "run-reveal",
        seq: 1,
        stateVersion: 0,
        timestamp: "2026-08-03T00:00:00.000Z",
        type: "accepted",
        payload: {
          kind: "accepted",
          turnId: "turn-1",
          sessionKey: "session-1",
        },
      },
      {
        runId: "run-reveal",
        seq: 2,
        stateVersion: 1,
        timestamp: "2026-08-03T00:00:01.000Z",
        type: "stage_changed",
        payload: {
          kind: "stage_changed",
          state: "running",
          stage: "正在生成答复",
        },
      },
    ] satisfies AssistantRunEvent[]);

    act(() =>
      root?.render(
        <Probe
          run={running}
          presentation={{
            runId: "run-reveal",
            lastSeq: 2,
            resyncFromSeq: null,
            pendingEvents: [],
            processItems: [],
            answer: "完整答复",
            answerComplete: false,
          }}
          presentationReveal={{
            runId: "run-reveal",
            answer: "完整",
            revealing: true,
          }}
        />,
      ),
    );

    expect(messages[1]?.content).toBe("完整");
    expect(messages[1]?.presentationStreaming).toBe(true);
    expect(restoreChatLineContent(messages[1]!)).toBe("完整答复");
  });

  it("keeps the bubble streaming while reveal drains after completion", () => {
    messages = [
      {
        role: "user",
        content: "你好",
        runId: "run-complete",
        turnId: "turn-1",
      },
      {
        role: "assistant",
        content: "",
        runId: "run-complete",
        turnId: "turn-1",
      },
    ];
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    const completed = replayAssistantRunEvents("run-complete", [
      {
        runId: "run-complete",
        seq: 1,
        stateVersion: 0,
        timestamp: "2026-08-03T00:00:00.000Z",
        type: "accepted",
        payload: {
          kind: "accepted",
          turnId: "turn-1",
          sessionKey: "session-1",
        },
      },
      {
        runId: "run-complete",
        seq: 2,
        stateVersion: 1,
        timestamp: "2026-08-03T00:00:01.000Z",
        type: "stage_changed",
        payload: {
          kind: "stage_changed",
          state: "running",
          stage: "正在生成答复",
        },
      },
      {
        runId: "run-complete",
        seq: 3,
        stateVersion: 2,
        timestamp: "2026-08-03T00:00:02.000Z",
        type: "content_delta",
        payload: { kind: "content_delta", delta: "完整答复" },
      },
      {
        runId: "run-complete",
        seq: 4,
        stateVersion: 3,
        timestamp: "2026-08-03T00:00:03.000Z",
        type: "completed",
        payload: { kind: "completed", messageId: "message-1" },
      },
    ] satisfies AssistantRunEvent[]);

    act(() =>
      root?.render(
        <Probe
          run={completed}
          presentation={{
            runId: "run-complete",
            lastSeq: 4,
            resyncFromSeq: null,
            pendingEvents: [],
            processItems: [],
            answer: "完整答复",
            answerComplete: true,
          }}
          presentationReveal={{
            runId: "run-complete",
            answer: "完整",
            revealing: true,
          }}
        />,
      ),
    );

    expect(messages[1]?.content).toBe("完整");
    expect(messages[1]?.presentationStreaming).toBe(true);
  });

  it("new_run_never_projects_previous_reveal_answer", () => {
    messages = [
      { role: "assistant", content: "上一轮完整回答", runId: "run-old" },
      { role: "user", content: "新问题", runId: "run-new" },
      { role: "assistant", content: "", runId: "run-new" },
    ];
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);

    const frameCallbacks = new Map<number, FrameRequestCallback>();
    let nextFrame = 1;
    const requestFrame = vi
      .spyOn(window, "requestAnimationFrame")
      .mockImplementation((callback) => {
        const frame = nextFrame;
        nextFrame += 1;
        frameCallbacks.set(frame, callback);
        return frame;
      });
    const cancelFrame = vi
      .spyOn(window, "cancelAnimationFrame")
      .mockImplementation((frame) => {
        frameCallbacks.delete(frame);
      });

    try {
      const oldRun = replayAssistantRunEvents("run-old", [
        {
          runId: "run-old",
          seq: 1,
          stateVersion: 0,
          timestamp: "2026-08-03T00:00:00.000Z",
          type: "accepted",
          payload: {
            kind: "accepted",
            turnId: "turn-old",
            sessionKey: "session-1",
          },
        },
        {
          runId: "run-old",
          seq: 2,
          stateVersion: 1,
          timestamp: "2026-08-03T00:00:01.000Z",
          type: "stage_changed",
          payload: {
            kind: "stage_changed",
            state: "running",
            stage: "正在生成答复",
          },
        },
        {
          runId: "run-old",
          seq: 3,
          stateVersion: 2,
          timestamp: "2026-08-03T00:00:02.000Z",
          type: "content_delta",
          payload: { kind: "content_delta", delta: "上一轮完整回答" },
        },
        {
          runId: "run-old",
          seq: 4,
          stateVersion: 3,
          timestamp: "2026-08-03T00:00:03.000Z",
          type: "completed",
          payload: { kind: "completed", messageId: "message-old" },
        },
      ] satisfies AssistantRunEvent[]);

      act(() =>
        root?.render(
          <RevealProjectionProbe
            run={oldRun}
            presentation={{
              runId: "run-old",
              lastSeq: 4,
              resyncFromSeq: null,
              pendingEvents: [],
              processItems: [],
              answer: "上一轮完整回答",
              answerComplete: false,
            }}
          />,
        ),
      );
      while (frameCallbacks.size > 0) {
        const callbacks = Array.from(frameCallbacks.values());
        frameCallbacks.clear();
        act(() => {
          callbacks.forEach((callback) => callback(16));
        });
      }
      expect(
        messages.find(
          (message) =>
            message.role === "assistant" && message.runId === "run-old",
        )?.content,
      ).toBe("上一轮完整回答");

      const newRun = replayAssistantRunEvents("run-new", [
        {
          runId: "run-new",
          seq: 1,
          stateVersion: 0,
          timestamp: "2026-08-03T00:00:04.000Z",
          type: "accepted",
          payload: {
            kind: "accepted",
            turnId: "turn-new",
            sessionKey: "session-1",
          },
        },
      ] satisfies AssistantRunEvent[]);

      act(() =>
        root?.render(
          <RevealProjectionProbe
            run={newRun}
            presentation={{
              runId: "run-new",
              lastSeq: 1,
              resyncFromSeq: null,
              pendingEvents: [],
              processItems: [],
              answer: "",
              answerComplete: false,
            }}
          />,
        ),
      );

      expect(
        messages.find(
          (message) =>
            message.role === "assistant" && message.runId === "run-old",
        )?.content,
      ).toBe("上一轮完整回答");
      expect(
        messages.find(
          (message) =>
            message.role === "assistant" && message.runId === "run-new",
        )?.content,
      ).toBe("");
    } finally {
      requestFrame.mockRestore();
      cancelFrame.mockRestore();
    }
  });

  it("terminal_recovery_uses_only_its_own_persisted_answer", () => {
    messages = [
      { role: "assistant", content: "上一轮完整回答", runId: "run-old" },
      { role: "user", content: "新问题", runId: "run-new" },
      { role: "assistant", content: "", runId: "run-new" },
    ];
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);

    const newRun = replayAssistantRunEvents("run-new", [
      {
        runId: "run-new",
        seq: 1,
        stateVersion: 0,
        timestamp: "2026-08-03T00:00:00.000Z",
        type: "accepted",
        payload: {
          kind: "accepted",
          turnId: "turn-new",
          sessionKey: "session-1",
        },
      },
      {
        runId: "run-new",
        seq: 2,
        stateVersion: 1,
        timestamp: "2026-08-03T00:00:01.000Z",
        type: "stage_changed",
        payload: {
          kind: "stage_changed",
          state: "running",
          stage: "正在生成答复",
        },
      },
      {
        runId: "run-new",
        seq: 3,
        stateVersion: 2,
        timestamp: "2026-08-03T00:00:02.000Z",
        type: "content_delta",
        payload: { kind: "content_delta", delta: "本轮持久化正文" },
      },
      {
        runId: "run-new",
        seq: 4,
        stateVersion: 3,
        timestamp: "2026-08-03T00:00:03.000Z",
        type: "completed",
        payload: { kind: "completed", messageId: "message-new" },
      },
    ] satisfies AssistantRunEvent[]);

    act(() => root?.render(<Probe run={newRun} presentation={null} />));

    expect(
      messages.find(
        (message) =>
          message.role === "assistant" && message.runId === "run-old",
      )?.content,
    ).toBe("上一轮完整回答");
    expect(
      messages.find(
        (message) =>
          message.role === "assistant" && message.runId === "run-new",
      )?.content,
    ).toBe("本轮持久化正文");
  });
});
