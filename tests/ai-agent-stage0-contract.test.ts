import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

const specPath = "docs/superpowers/specs/ai-agent-system-remediation.md";
const matrixPath = "docs/audits/2026-06-20-ai-agent-issue-matrix.md";
const readinessPath = "docs/audits/2026-06-20-ai-agent-current-readiness.md";

describe("AI agent stage 0 baseline contracts", () => {
  it("documents the full audit scope and future architecture entities", () => {
    const spec = read(specPath);

    for (const dimension of [
      "长对话",
      "复杂推理",
      "subagent",
      "工具",
      "skills",
      "检索",
      "文件权限",
      "agent 权限",
      "沙箱",
      "前端协作状态",
    ]) {
      expect(spec).toContain(dimension);
    }

    for (const entity of [
      "ToolExecutionPipeline",
      "PermissionDecisionEngine",
      "ConversationMemory",
      "DeliberationState",
      "WritingState",
      "ResearchState",
      "EvidencePipeline",
      "SubAgentCoordinator",
      "SkillTrustPolicy",
      "SandboxProfile",
    ]) {
      expect(spec).toContain(entity);
    }

    for (let phase = 1; phase <= 10; phase += 1) {
      expect(spec).toContain(`阶段 ${phase}`);
    }
  });

  it("keeps the issue matrix evidence-based and stage-targeted", () => {
    const matrix = read(matrixPath);

    for (const header of [
      "ID",
      "来源",
      "维度",
      "组件",
      "声明",
      "证据",
      "严重度",
      "状态",
      "目标阶段",
      "验收方式",
    ]) {
      expect(matrix).toContain(header);
    }

    for (const status of ["成立", "部分成立", "不成立", "需实验验证"]) {
      expect(matrix).toContain(status);
    }

    for (const source of ["DeepSeek", "MIMO", "人工复核", "源码复核"]) {
      expect(matrix).toContain(source);
    }

    for (const component of [
      "run.rs",
      "tool_policy.rs",
      "agent_permissions.rs",
      "harness_confirm.rs",
      "model_gateway",
      "skills",
      "retrieval_broker",
    ]) {
      expect(matrix).toContain(component);
    }
  });

  it("records baseline decisions instead of implementing later fixes in stage 0", () => {
    const matrix = read(matrixPath);

    expect(matrix).toContain("join_all");
    expect(matrix).toContain("ToolPolicy");
    expect(matrix).toContain("permission preflight");
    expect(matrix).toContain("Anthropic");
    expect(matrix).toContain("useAssistantTasks");
    expect(matrix).toContain("useAgentTaskStatus");
    expect(matrix).toContain("raw checkpoint");
  });

  it("keeps issue matrix rows parseable as markdown tables", () => {
    const matrix = read(matrixPath);
    const tableLines = matrix
      .split("\n")
      .filter((line) => line.startsWith("| A"));
    const headerCells = matrix
      .split("\n")
      .find((line) => line.startsWith("| ID"))
      ?.split("|").length;

    expect(headerCells).toBeGreaterThan(0);
    for (const line of tableLines) {
      expect(line.split("|").length, line).toBe(headerCells);
    }
  });

  it("keeps the historical baseline separate from current readiness", () => {
    const readiness = read(readinessPath);

    expect(readiness).toContain("阶段 0 历史基线");
    expect(readiness).toContain("AAR-001");
    expect(readiness).toContain("已修复");
    expect(readiness).toContain("最小实现");
    expect(readiness).toContain("真实 LLM");
    expect(readiness).toContain("前端新增字段均为可选字段");
  });
});
