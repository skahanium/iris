import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("performance guide contract", () => {
  it("documents Iris performance optimization baseline scenarios", () => {
    const guide = read("docs/ops/performance-guide.md");

    expect(guide).toContain("Iris Performance Optimization Baselines");
    expect(guide).toContain("离线冷启动");
    expect(guide).toContain("AI 长流式输出");
    expect(guide).toContain("已打开 Tab 快速回切");
    expect(guide).toContain("10000+ 文件库");
  });

  it("requires before and after measurements before claiming performance completion", () => {
    const guide = read("docs/ops/performance-guide.md");

    expect(guide).toContain("改前");
    expect(guide).toContain("改后");
    expect(guide).toContain(">50ms long task");
    expect(guide).toContain("React Profiler");
    expect(guide).toContain("DevTools Performance");
  });

  it("states that traces must not include user note content or credentials", () => {
    const guide = read("docs/ops/performance-guide.md");

    expect(guide).toContain("不得包含用户笔记正文");
    expect(guide).toContain("不得包含 API Key");
    expect(guide).toContain("不得包含解密后的涉密内容");
  });
});
