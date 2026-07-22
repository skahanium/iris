import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";

import { useAssistantRunTranscript } from "@/components/ai/hooks/useAssistantRunTranscript";
import type { ChatLine } from "@/components/ai/AiMessageList";
import { replayAssistantRunEvents } from "@/lib/assistant-run-events";
import type { AssistantRunEvent } from "@/types/ai";

let root: Root | null = null;
let host: HTMLDivElement | null = null;
let messages: ChatLine[] = [];

function Probe({ run }: { run: ReturnType<typeof replayAssistantRunEvents> }) {
  useAssistantRunTranscript({
    run,
    messages,
    setMessages: (updater) => {
      messages = typeof updater === "function" ? updater(messages) : updater;
    },
    setStreaming: () => undefined,
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
});

describe("useAssistantRunTranscript", () => {
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
        processItems: [
          { id: "stage:2", label: "正在准备" },
          { id: "stage:3", label: "正在生成答复" },
        ],
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
});
