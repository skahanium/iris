import { describe, expect, it } from "vitest";

import { projectAssistantProcessEvents } from "@/lib/assistant-process";
import {
  createAssistantRunEventState,
  reduceAssistantRunEvent,
} from "@/lib/assistant-run-events";
import type { AssistantRunEvent } from "@/types/ai";

const runId = "run-process-001";
const timestamp = "2026-07-22T08:00:00.000Z";

function event<Type extends AssistantRunEvent["type"]>(
  seq: number,
  type: Type,
  payload: Extract<AssistantRunEvent, { type: Type }>["payload"],
  eventTimestamp = timestamp,
): Extract<AssistantRunEvent, { type: Type }> {
  return {
    runId,
    seq,
    stateVersion: seq,
    type,
    timestamp: eventTimestamp,
    payload,
  } as Extract<AssistantRunEvent, { type: Type }>;
}

describe("Assistant Run 处理过程投影", () => {
  it("用临时摘要快照更新 UI，并由持久化摘要提交它", () => {
    let state = createAssistantRunEventState(runId);
    state = reduceAssistantRunEvent(
      state,
      event(1, "accepted", {
        kind: "accepted",
        turnId: "turn-001",
        sessionKey: "session-001",
      }),
    );
    state = reduceAssistantRunEvent(
      state,
      event(0, "reasoning_summary", {
        kind: "reasoning_summary",
        summaryId: "summary-001",
        text: "正在比较检索到的证据。",
      }),
    );

    expect(state.reasoningSummaries).toEqual([
      {
        summaryId: "summary-001",
        text: "正在比较检索到的证据。",
      },
    ]);
    expect(state.events).toHaveLength(1);

    state = reduceAssistantRunEvent(
      state,
      event(2, "reasoning_summary", {
        kind: "reasoning_summary",
        summaryId: "summary-001",
        text: "已比较检索到的证据，准备作答。",
      }),
    );

    expect(state.reasoningSummaries).toEqual([
      {
        summaryId: "summary-001",
        text: "已比较检索到的证据，准备作答。",
      },
    ]);
    expect(state.events.map((item) => item.type)).toEqual([
      "accepted",
      "reasoning_summary",
    ]);
  });

  it("将工具开始和完成合并为一条不含参数或原始输出的安全过程项", () => {
    const items = projectAssistantProcessEvents([
      event(1, "accepted", {
        kind: "accepted",
        turnId: "turn-001",
        sessionKey: "session-001",
      }),
      event(2, "tool_started", {
        kind: "tool_started",
        capability: "web_search",
        toolCallId: "tool-001",
      }),
      event(
        3,
        "tool_completed",
        {
          kind: "tool_completed",
          capability: "web_search",
          toolCallId: "tool-001",
          summary: "工具调用完成",
        },
        "2026-07-22T08:00:01.250Z",
      ),
    ]);

    expect(items).toEqual([
      {
        id: "tool:tool-001",
        kind: "tool",
        label: "联网搜索",
        status: "completed",
        createdAt: Date.parse(timestamp),
        durationMs: 1250,
      },
    ]);
  });

  it("历史回放优先使用工具完成事件记录的真实耗时", () => {
    const items = projectAssistantProcessEvents([
      event(1, "tool_started", {
        kind: "tool_started",
        capability: "web_search",
        toolCallId: "tool-precise-duration",
      }),
      event(2, "tool_completed", {
        kind: "tool_completed",
        capability: "web_search",
        toolCallId: "tool-precise-duration",
        summary: "工具调用完成",
        durationMs: 6_700,
      } as Extract<AssistantRunEvent["payload"], { kind: "tool_completed" }>),
    ]);

    expect(items[0]?.durationMs).toBe(6_700);
  });

  it("保留阶段和 provider 显式摘要，但绝不把最终正文投影为过程内容", () => {
    const items = projectAssistantProcessEvents([
      event(1, "accepted", {
        kind: "accepted",
        turnId: "turn-001",
        sessionKey: "session-001",
      }),
      event(2, "stage_changed", {
        kind: "stage_changed",
        state: "preparing",
        stage: "正在准备工具执行",
      }),
      event(3, "reasoning_summary", {
        kind: "reasoning_summary",
        summaryId: "summary-001",
        text: "先核验来源，再组织答案。",
      }),
      event(4, "content_delta", {
        kind: "content_delta",
        delta: "这段最终正文不得进入处理过程。",
      }),
    ]);

    expect(items).toEqual([
      {
        id: "stage:2",
        kind: "stage",
        label: "正在准备工具执行",
        status: "completed",
        createdAt: Date.parse(timestamp),
      },
      {
        id: "reasoning:summary-001",
        kind: "reasoning_summary",
        label: "先核验来源，再组织答案。",
        status: "completed",
        createdAt: Date.parse(timestamp),
      },
    ]);
  });
});
