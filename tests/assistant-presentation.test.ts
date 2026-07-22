import { describe, expect, it } from "vitest";

import {
  createAssistantPresentationState,
  reduceAssistantPresentationEvent,
  type AssistantPresentationEvent,
} from "@/lib/assistant-presentation";

const runId = "run-presentation-001";

function event(
  presentationSeq: number,
  type: AssistantPresentationEvent["type"],
  payload: AssistantPresentationEvent["payload"],
): AssistantPresentationEvent {
  return {
    runId,
    presentationSeq,
    elapsedMs: presentationSeq * 10,
    type,
    payload,
  };
}

describe("Assistant 实时展示事件", () => {
  it("在缺少序号时不拼接正文，并在缺口补齐后按严格顺序消费", () => {
    let state = createAssistantPresentationState(runId);
    state = reduceAssistantPresentationEvent(
      state,
      event(2, "answer_delta", { kind: "answer_delta", delta: "答" }),
    );

    expect(state.answer).toBe("");
    expect(state.resyncFromSeq).toBe(1);

    state = reduceAssistantPresentationEvent(
      state,
      event(1, "process_started", {
        kind: "process_started",
        itemId: "tool:web-1",
        itemKind: "tool",
        label: "联网搜索",
      }),
    );

    expect(state.lastSeq).toBe(2);
    expect(state.answer).toBe("答");
    expect(state.resyncFromSeq).toBeNull();
  });

  it("answer_reset 会清空尚未展示的实时正文", () => {
    let state = createAssistantPresentationState(runId);
    state = reduceAssistantPresentationEvent(
      state,
      event(1, "answer_delta", { kind: "answer_delta", delta: "候选前言" }),
    );
    state = reduceAssistantPresentationEvent(
      state,
      event(2, "answer_reset", { kind: "answer_reset" }),
    );

    expect(state.answer).toBe("");
  });

  it("新过程项默认 running，新 stage 会结束上一个 stage", () => {
    let state = createAssistantPresentationState(runId);
    state = reduceAssistantPresentationEvent(
      state,
      event(1, "process_started", {
        kind: "process_started",
        itemId: "stage:1",
        itemKind: "stage",
        label: "正在准备",
      }),
    );
    expect(state.processItems[0]?.status).toBe("running");

    state = reduceAssistantPresentationEvent(
      state,
      event(2, "process_started", {
        kind: "process_started",
        itemId: "stage:2",
        itemKind: "stage",
        label: "正在生成答复",
      }),
    );
    expect(state.processItems[0]?.status).toBe("completed");
    expect(state.processItems[1]?.status).toBe("running");
  });

  it("answer_complete 会结束仍在 running 的工具项", () => {
    let state = createAssistantPresentationState(runId);
    state = reduceAssistantPresentationEvent(
      state,
      event(1, "process_started", {
        kind: "process_started",
        itemId: "tool:web-1",
        itemKind: "tool",
        label: "联网搜索",
      }),
    );
    state = reduceAssistantPresentationEvent(
      state,
      event(2, "answer_complete", { kind: "answer_complete" }),
    );
    expect(state.processItems[0]?.status).toBe("completed");
  });
});
