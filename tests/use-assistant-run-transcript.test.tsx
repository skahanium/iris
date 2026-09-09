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
  const reveal = useAssistantAnswerReveal({
    ...presentation,
    answerComplete: presentation.answerComplete || run.state === "completed",
  });
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
  it("连续候选 delta 在正式提交前始终不可见", () => {
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
    expect(messages[1]?.content).toBe("");

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
    expect(messages[1]?.content).toBe("");
  });

  it("answerComplete 先到时仍等待 durable completed", () => {
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
    expect(streaming).toBe(true);
  });

  it("HR-4：终态遇到展示序号缺口时以同 Run 的可靠正文收敛", () => {
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

  it("提交前取消只显示取消通知，不发布候选正文", () => {
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

    expect(messages[1]?.role).toBe("system");
    expect(messages[1]?.content).toBe("本次回答已取消。");
    expect(
      messages.some((message) => message.content.includes("发送继续")),
    ).toBe(false);
    expect(
      messages[1]?.processItems?.some((item) => item.label === "答复完毕") ??
        false,
    ).toBe(false);
  });

  it("提交前失败不发布候选正文，也不显示答复完毕", () => {
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

    expect(messages).toHaveLength(1);
    expect(messages[0]?.role).toBe("user");
  });

  it("展示序号缺口不得把未提交候选写入正文", () => {
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

    expect(messages[1]?.content).toBe("");
  });

  it("waits for completion before exposing a durable content delta", () => {
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
        content: "",
        runId: "run-1",
        turnId: "turn-1",
        processItems: [{ id: "stage:3", label: "正在生成答复" }],
      },
    ]);
  });

  it("rebuilds the missing assistant slot for a recovered user-only Run", () => {
    messages = [
      {
        role: "user",
        content: "最近有什么新上映的电影吗？",
        runId: "run-recovered",
        turnId: "turn-recovered",
      },
    ];
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);

    act(() =>
      root?.render(
        <Probe
          run={replayAssistantRunEvents("run-recovered", [
            {
              runId: "run-recovered",
              seq: 1,
              stateVersion: 0,
              timestamp: "2026-08-25T13:22:00.000Z",
              type: "accepted",
              payload: {
                kind: "accepted",
                turnId: "turn-recovered",
                sessionKey: "session-1",
              },
            },
            {
              runId: "run-recovered",
              seq: 2,
              stateVersion: 1,
              timestamp: "2026-08-25T13:22:01.000Z",
              type: "stage_changed",
              payload: {
                kind: "stage_changed",
                state: "preparing",
                stage: "正在准备",
              },
            },
            {
              runId: "run-recovered",
              seq: 3,
              stateVersion: 2,
              timestamp: "2026-08-25T13:22:02.000Z",
              type: "stage_changed",
              payload: {
                kind: "stage_changed",
                state: "running",
                stage: "正在等待补充信息",
              },
            },
            {
              runId: "run-recovered",
              seq: 4,
              stateVersion: 3,
              timestamp: "2026-08-25T13:22:03.000Z",
              type: "input_required",
              payload: {
                kind: "input_required",
                inputId: "location-run-recovered",
                inputKind: "location",
                fields: ["city"],
                prompt: "请告诉我需要查询的城市",
              },
            },
            {
              runId: "run-recovered",
              seq: 5,
              stateVersion: 4,
              timestamp: "2026-08-25T13:22:04.000Z",
              type: "input_provided",
              payload: {
                kind: "input_provided",
                inputId: "location-run-recovered",
                values: { city: "上海" },
              },
            },
            {
              runId: "run-recovered",
              seq: 6,
              stateVersion: 5,
              timestamp: "2026-08-25T13:22:05.000Z",
              type: "stage_changed",
              payload: {
                kind: "stage_changed",
                state: "running",
                stage: "正在生成答复",
              },
            },
            {
              runId: "run-recovered",
              seq: 7,
              stateVersion: 5,
              timestamp: "2026-08-25T13:22:06.000Z",
              type: "content_delta",
              payload: { kind: "content_delta", delta: "以下是近期新片建议。" },
            },
            {
              runId: "run-recovered",
              seq: 8,
              stateVersion: 6,
              timestamp: "2026-08-25T13:22:07.000Z",
              type: "completed",
              payload: { kind: "completed", messageId: "message-recovered" },
            },
          ] satisfies AssistantRunEvent[])}
        />,
      ),
    );

    const recoveredAssistantMessages = messages.filter(
      (message) =>
        message.role === "assistant" && message.runId === "run-recovered",
    );
    expect(recoveredAssistantMessages).toHaveLength(1);
    expect(recoveredAssistantMessages).toMatchObject([
      {
        content: "以下是近期新片建议。",
        turnId: "turn-recovered",
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
          run={replayCompleteFixture("run-1", [
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
            {
              runId: "run-1",
              seq: 3,
              stateVersion: 1,
              timestamp: "2026-07-13T12:00:02Z",
              type: "completed",
              payload: { kind: "completed", messageId: "answer-1" },
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

  it("HR-4：无同 Run 用户行的迟到终态不污染当前会话", () => {
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
            {
              runId: "run-late",
              seq: 3,
              stateVersion: 1,
              timestamp: "2026-07-13T12:00:02.000Z",
              type: "completed",
              payload: { kind: "completed", messageId: "message-late" },
            },
          ] satisfies AssistantRunEvent[])}
        />,
      ),
    );

    expect(messages[1]?.content).toBe("当前回答");
    expect(messages).toHaveLength(2);
    expect(messages.every((message) => message.runId === "run-2")).toBe(true);
  });

  it("ignores legacy presentation playback before commitment", () => {
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

    expect(messages[1]?.content).toBe("");
    expect(messages[1]?.presentationStreaming).toBe(true);
    expect(restoreChatLineContent(messages[1]!)).toBe("");
  });

  it("sets the immutable full target for bubble-local playback on commitment", () => {
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
    const completed = replayCompleteFixture("run-complete", [
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

    expect(messages[1]?.content).toBe("完整答复");
    expect(messages[1]?.answerPresentation?.complete).toBe(true);
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
          timestamp: "2026-08-03T00:00:00.500Z",
          type: "stage_changed",
          payload: {
            kind: "stage_changed",
            state: "preparing",
            stage: "正在准备",
          },
        },
        {
          runId: "run-old",
          seq: 3,
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
          seq: 4,
          stateVersion: 2,
          timestamp: "2026-08-03T00:00:02.000Z",
          type: "content_delta",
          payload: { kind: "content_delta", delta: "上一轮完整回答" },
        },
        {
          runId: "run-old",
          seq: 5,
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
              lastSeq: 5,
              resyncFromSeq: null,
              pendingEvents: [],
              processItems: [],
              answer: "上一轮完整回答",
              answerComplete: false,
            }}
          />,
        ),
      );
      let frameTime = 0;
      while (frameCallbacks.size > 0 && frameTime < 120_000) {
        frameTime += 1000 / 60;
        const callbacks = Array.from(frameCallbacks.values());
        frameCallbacks.clear();
        act(() => {
          callbacks.forEach((callback) => callback(frameTime));
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

    const newRun = replayCompleteFixture("run-new", [
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

function replayCompleteFixture(runId: string, events: AssistantRunEvent[]) {
  const accepted = events[0]!;
  return replayAssistantRunEvents(
    runId,
    [
      accepted,
      {
        ...accepted,
        type: "stage_changed",
        payload: {
          kind: "stage_changed",
          state: "preparing",
          stage: "正在准备",
        },
      },
      {
        ...accepted,
        type: "stage_changed",
        payload: { kind: "stage_changed", state: "running", stage: "正在处理" },
      },
      ...events.slice(1),
    ].map((event, index) => ({
      ...event,
      seq: index + 1,
    })) as AssistantRunEvent[],
  );
}
