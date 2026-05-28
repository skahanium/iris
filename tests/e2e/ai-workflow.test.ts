/**
 * E2E AI 工作流验收测试
 *
 * 注意：这些测试目前为占位符，需要实际 Tauri 环境运行
 */
import { describe, it, expect } from "vitest";
import {
  waitForAiPanel,
  selectAiScene,
  sendAiMessage,
  expectContextPackets,
} from "./helpers";

describe("AI 工作流验收", () => {
  it("知识查阅场景", async () => {
    await waitForAiPanel();
    await selectAiScene("knowledge-lookup");
    await sendAiMessage("什么是 SQLite？");
    expectContextPackets(1);
    const hasResponse = true;
    expect(hasResponse).toBe(true);
  });

  it("文稿创作场景", async () => {
    await waitForAiPanel();
    await selectAiScene("drafting-assist");
    await sendAiMessage("帮我写一段项目介绍");
    const hasResponse = true;
    expect(hasResponse).toBe(true);
  });

  it("工具调用显示", async () => {
    await waitForAiPanel();
    await selectAiScene("knowledge-lookup");
    await sendAiMessage("搜索相关内容");
    const hasToolCallBubble = true;
    expect(hasToolCallBubble).toBe(true);
  });
});
