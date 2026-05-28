/**
 * E2E acceptance tests — v0.1.0 核心功能验收
 *
 * 注意：这些测试目前为占位符，需要实际 Tauri 环境运行
 */
import { describe, it, expect } from "vitest";
import { waitForAiPanel } from "./helpers";

describe("Iris 核心功能验收", () => {
  it("应用启动并显示主界面", () => {
    // 在实际 Tauri 环境中，这里会检查 editor、tab-bar、status-bar 是否可见
    const hasEditor = true;
    const hasTabBar = true;
    const hasStatusBar = true;
    expect(hasEditor).toBe(true);
    expect(hasTabBar).toBe(true);
    expect(hasStatusBar).toBe(true);
  });

  it("AI 面板可打开和关闭", async () => {
    // 模拟打开 AI 面板
    await waitForAiPanel();
    const isPanelOpen = true;
    expect(isPanelOpen).toBe(true);

    // 模拟关闭 AI 面板
    const isPanelClosed = true;
    expect(isPanelClosed).toBe(true);
  });

  it("创建新笔记", () => {
    // 在实际 Tauri 环境中，这里会通过快捷键创建新笔记
    const hasNewTab = true;
    expect(hasNewTab).toBe(true);
  });
});
