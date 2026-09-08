import { describe, expect, it } from "vitest";
import {
  createAssistantRunEventState,
  reduceAssistantRunEvent,
  userVisibleRunFailureMessage,
} from "@/lib/assistant-run-events";

describe("续答失败的真实进度", () => {
  it("搜索刚派发不能被计为已经返回", () => {
    const state = reduceAssistantRunEvent(createAssistantRunEventState("run"), {
      runId: "run",
      seq: 1,
      stateVersion: 1,
      timestamp: "2026-09-08T00:00:00Z",
      type: "tool_started",
      payload: {
        kind: "tool_started",
        capability: "web_search",
        toolCallId: "search",
      },
    });
    expect(state.webSearched).toBe(false);
  });

  it.each([false, true])(
    "笼统模型错误不能断言检索完成或要求换模型：%s",
    (searched) => {
      const message = userVisibleRunFailureMessage(
        "agent_run_provider_unavailable",
        "模型服务暂时不可用，请稍后重试",
        searched,
      );
      expect(message).not.toMatch(/检索已完成|更换模型|服务暂时不可用/);
      expect(message).toContain("答复生成失败");
    },
  );
});
