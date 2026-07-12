import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

const specRoot = "docs/agent-harness-refactor";
const matrixPath = "docs/audits/2026-06-20-ai-agent-issue-matrix.md";
const readinessPath = "docs/audits/2026-06-20-ai-agent-current-readiness.md";

describe("AI agent stage 0 baseline contracts", () => {
  it("uses the agent-harness-refactor document suite as the sole target specification", () => {
    const readme = read(`${specRoot}/README.md`);
    const architecture = read(`${specRoot}/02-target-architecture.md`);
    const lifecycle = read(`${specRoot}/03-lifecycle-and-evidence.md`);
    const policy = read(`${specRoot}/04-policy-and-security.md`);
    const migration = read(`${specRoot}/07-api-and-data-migration.md`);
    const plan = read(`${specRoot}/08-implementation-plan.md`);

    expect(readme).toContain("状态：**目标规格，尚未实施**");
    expect(readme).toContain("一个对话入口、一个 Run 状态机、一个权限策略引擎");
    expect(readme).toContain("课题研究专用工作流");
    expect(architecture).toContain("ExecutionEnvelope");
    expect(lifecycle).toContain("AssistantRunEvent");
    expect(policy).toContain("PolicyDecisionEngine");
    expect(migration).toContain("assistant_run_start");

    for (let phase = 0; phase <= 10; phase += 1) {
      expect(plan).toContain(`阶段 ${phase}`);
    }
  });

  it("records the legacy execution chain before later phases remove it", () => {
    const commands = read("src-tauri/src/lib.rs");
    const ipc = read("src/lib/ipc.ts");
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");

    for (const command of [
      "assistant_execute",
      "context_assemble",
      "ai_send_message",
      "harness_resume",
      "agent_task_resume",
      "research_execute",
    ]) {
      expect(commands).toContain(command);
      expect(ipc).toContain(command);
    }

    expect(panel).toContain("useAssistantTasks");
    expect(panel).toContain("useAssistantHarnessResume");
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
