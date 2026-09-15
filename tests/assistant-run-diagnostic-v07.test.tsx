import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { AssistantRunDiagnosticReport } from "@/components/ai/AssistantRunDiagnostic";
import type {
  DiagnosticFinding,
  DiagnosticIssueClass,
  DiagnosticReport,
} from "@/types/ai";

function finding(
  statement: string,
  issueClass: DiagnosticIssueClass,
  extras: Partial<DiagnosticFinding> = {},
): DiagnosticFinding {
  return {
    statement,
    discovery: {
      module: extras.discovery?.module ?? "M05",
      component: extras.discovery?.component ?? "C14",
      toolInstance: extras.discovery?.toolInstance,
      callId: extras.discovery?.callId ?? "call-1",
      attemptId: extras.discovery?.attemptId ?? "attempt-1",
      modelTurn: extras.discovery?.modelTurn ?? 1,
    },
    issueClass,
    confirmedSource: extras.confirmedSource,
  };
}

function report(overrides: Partial<DiagnosticReport>): DiagnosticReport {
  return {
    schemaVersion: 1,
    runId: "run-v07",
    inputRevision: "rev-1",
    parentRunId: null,
    childRunId: null,
    recordCompleteness: "complete",
    attributionStatus: "unattributed",
    auditHealth: { persistFailed: false },
    headline: "诊断不完整。",
    impact: "不能当作执行成功。",
    recoveryState: "未知。",
    knownFacts: [],
    directFailures: [],
    recoveryResults: [],
    pendingRootCauses: [],
    expectedRestrictions: [],
    evidenceGaps: [],
    path: [
      {
        module: "M05",
        component: "C14",
        callId: "call-1",
        attemptId: "attempt-1",
        modelTurn: 1,
      },
    ],
    ...overrides,
  };
}

const SECRET_NOTE = "用户笔记正文不得出现";
const SECRET_KEY = "sk-live-secret-key";

function assertLocatable(
  text: string,
  expected: {
    discovery: string;
    issueClass: string;
    attribution: string;
    recovery: string;
    impact: string;
    headlineForbidden?: string[];
  },
) {
  expect(text).toContain(expected.discovery);
  expect(text).toContain(expected.issueClass);
  expect(text).toContain(expected.attribution);
  expect(text).toContain(expected.recovery);
  expect(text).toContain(expected.impact);
  expect(text).toContain("路径：");
  expect(text).not.toContain(SECRET_NOTE);
  expect(text).not.toContain(SECRET_KEY);
  expect(text).not.toContain("https://example.invalid/private");
  for (const forbidden of expected.headlineForbidden ?? [
    "未发现可证实的故障",
  ]) {
    expect(text).not.toContain(forbidden);
  }
}

describe("V07 diagnostic locatability projection", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  async function renderReport(value: DiagnosticReport) {
    await act(async () => {
      root.render(<AssistantRunDiagnosticReport report={value} />);
    });
    return host.querySelector(
      '[data-testid="assistant-run-diagnostic-report"]',
    ) as HTMLElement;
  }

  it("internal contract error: missing request_sent is a gap not a model fault", async () => {
    const node = await renderReport(
      report({
        recordCompleteness: "missing-events",
        attributionStatus: "unattributed",
        headline: "出站层不完整，请求未证实已发出。",
        impact: "不能把业务层生成当成已发送。",
        recoveryState: "未知。",
        knownFacts: [
          finding("出站层 generated 已记录", "internal-contract", {
            discovery: {
              module: "M04",
              component: "C11",
              callId: "call-1",
              attemptId: "attempt-1",
              modelTurn: 1,
            },
          }),
        ],
        evidenceGaps: [
          finding("缺少 request_sent，未发送", "diagnostic-gap", {
            discovery: {
              module: "M04",
              component: "C11",
              callId: "call-1",
              attemptId: "attempt-1",
              modelTurn: 1,
            },
          }),
        ],
        path: [
          {
            module: "M04",
            component: "C11",
            callId: "call-1",
            attemptId: "attempt-1",
            modelTurn: 1,
          },
        ],
      }),
    );
    assertLocatable(node.textContent ?? "", {
      discovery: "M04 → C11 → call-1/attempt-1",
      issueClass: "诊断缺口",
      attribution: "○ 无法归因",
      recovery: "恢复：未知。",
      impact: "不能把业务层生成当成已发送",
    });
    expect(node.textContent).toContain("内部合同");
    expect(node.textContent).not.toContain("模型行为");
  });

  it("external failure: provider timeout is not a confirmed tool defect", async () => {
    const node = await renderReport(
      report({
        attributionStatus: "confirmed",
        headline: "外部服务超时。",
        impact: "本次调用未完成。",
        recoveryState: "未证实恢复。",
        directFailures: [
          finding("供应商超时", "external-service", {
            confirmedSource: "external-service",
            discovery: {
              module: "M04",
              component: "C11",
              callId: "call-timeout",
              attemptId: "attempt-1",
              modelTurn: 2,
            },
          }),
        ],
        path: [
          {
            module: "M04",
            component: "C11",
            callId: "call-timeout",
            attemptId: "attempt-1",
            modelTurn: 2,
          },
        ],
      }),
    );
    assertLocatable(node.textContent ?? "", {
      discovery: "M04 → C11 → call-timeout/attempt-1",
      issueClass: "外部服务",
      attribution: "● 已证实",
      recovery: "恢复：未证实恢复。",
      impact: "本次调用未完成",
    });
    expect(node.textContent).not.toContain("工具执行");
  });

  it("expected restriction: capability blocked is not shown as a program error", async () => {
    const node = await renderReport(
      report({
        attributionStatus: "unattributed",
        headline: "本次能力按合同拒绝，不是程序错误。",
        impact: "用户可见限制，不得记为故障。",
        recoveryState: "无需恢复。",
        expectedRestrictions: [
          finding("工具不可用，能力被阻止", "expected-restriction", {
            discovery: {
              module: "M05",
              component: "C14",
              callId: "call-1",
              attemptId: "attempt-1",
              modelTurn: 1,
            },
          }),
        ],
      }),
    );
    assertLocatable(node.textContent ?? "", {
      discovery: "M05 → C14 → call-1/attempt-1",
      issueClass: "预期限制",
      attribution: "○ 无法归因",
      recovery: "恢复：无需恢复。",
      impact: "用户可见限制",
    });
    expect(node.textContent).toContain("预期限制");
    expect(node.textContent).not.toContain("未发现可证实的故障");
    expect(node.textContent).not.toContain("工具执行");
  });

  it("recovery after failure keeps the original failure and does not treat recovery_exhausted as root cause", async () => {
    const node = await renderReport(
      report({
        attributionStatus: "confirmed",
        headline: "工具 web_fetch 执行失败。",
        impact: "本次工具调用未完成。",
        recoveryState: "恢复耗尽已记录，原始失败仍可查看。",
        directFailures: [
          finding("工具 web_fetch 执行失败", "tool-execution", {
            confirmedSource: "tool-execution",
            discovery: {
              module: "M05",
              component: "C14",
              toolInstance: "web_fetch",
              callId: "call-fetch",
              attemptId: "attempt-1",
              modelTurn: 1,
            },
          }),
        ],
        recoveryResults: [
          finding("恢复耗尽，这是恢复结果而不是根因", "internal-contract"),
        ],
        path: [
          {
            module: "M05",
            component: "C14",
            toolInstance: "web_fetch",
            callId: "call-fetch",
            attemptId: "attempt-1",
            modelTurn: 1,
          },
        ],
      }),
    );
    const text = node.textContent ?? "";
    assertLocatable(text, {
      discovery: "M05 → C14 → call-fetch/attempt-1 · web_fetch",
      issueClass: "工具执行",
      attribution: "● 已证实",
      recovery: "恢复：恢复耗尽已记录，原始失败仍可查看。",
      impact: "本次工具调用未完成",
    });
    expect(text).toContain("直接失败");
    expect(text).toContain("恢复结果");
    expect(text.indexOf("直接失败")).toBeLessThan(text.indexOf("恢复结果"));
    expect(text).not.toContain('confirmedSource": "recovery_exhausted');
  });

  it("partial multi-path success: four outbound layers remain visible beside one tool failure", async () => {
    const node = await renderReport(
      report({
        attributionStatus: "confirmed",
        headline: "工具 web_fetch 执行失败。",
        impact: "搜索路径已发出，抓取路径失败。",
        recoveryState: "未证实全部恢复。",
        knownFacts: [
          finding(
            "出站层 generated 已记录，协议 openai_chat_completions",
            "internal-contract",
            {
              discovery: {
                module: "M04",
                component: "C11",
                callId: "call-search",
                attemptId: "attempt-1",
                modelTurn: 1,
              },
            },
          ),
          finding("出站层 serialized 已记录", "internal-contract", {
            discovery: {
              module: "M04",
              component: "C11",
              callId: "call-search",
              attemptId: "attempt-1",
              modelTurn: 1,
            },
          }),
          finding("出站层 request_sent 已记录", "internal-contract", {
            discovery: {
              module: "M04",
              component: "C11",
              callId: "call-search",
              attemptId: "attempt-1",
              modelTurn: 1,
            },
          }),
          finding("出站层 provider_returned 已记录", "internal-contract", {
            discovery: {
              module: "M04",
              component: "C11",
              callId: "call-search",
              attemptId: "attempt-1",
              modelTurn: 1,
            },
          }),
        ],
        directFailures: [
          finding("工具 web_fetch 执行失败", "tool-execution", {
            confirmedSource: "tool-execution",
            discovery: {
              module: "M05",
              component: "C14",
              toolInstance: "web_fetch",
              callId: "call-fetch",
              attemptId: "attempt-2",
              modelTurn: 1,
            },
          }),
        ],
        path: [
          {
            module: "M04",
            component: "C11",
            callId: "call-search",
            attemptId: "attempt-1",
            modelTurn: 1,
          },
          {
            module: "M05",
            component: "C14",
            toolInstance: "web_fetch",
            callId: "call-fetch",
            attemptId: "attempt-2",
            modelTurn: 1,
          },
        ],
      }),
    );
    const text = node.textContent ?? "";
    expect(text).toContain("generated");
    expect(text).toContain("serialized");
    expect(text).toContain("request_sent");
    expect(text).toContain("provider_returned");
    expect(text).toContain("M04 → C11 → call-search/attempt-1");
    expect(text).toContain("M05 → C14 → call-fetch/attempt-2 · web_fetch");
    expect(text).toContain("● 已证实");
    expect(text).toContain("搜索路径已发出，抓取路径失败");
    expect(text).not.toContain(SECRET_KEY);
  });

  it("multiple independent faults are not collapsed into a single first error", async () => {
    const node = await renderReport(
      report({
        attributionStatus: "multiple-causes",
        headline: "存在多个独立问题，不能压成单一首错。",
        impact: "各失败需分别查看。",
        recoveryState: "未证实全部恢复。",
        directFailures: [
          finding("工具 web_fetch 执行失败", "tool-execution", {
            discovery: {
              module: "M05",
              component: "C14",
              toolInstance: "web_fetch",
              callId: "call-fetch",
              attemptId: "attempt-fetch",
              modelTurn: 1,
            },
          }),
          finding("工具 web_search 执行失败", "tool-execution", {
            discovery: {
              module: "M05",
              component: "C14",
              toolInstance: "web_search",
              callId: "call-search",
              attemptId: "attempt-search",
              modelTurn: 1,
            },
          }),
        ],
        path: [
          {
            module: "M05",
            component: "C14",
            toolInstance: "web_fetch",
            callId: "call-fetch",
            attemptId: "attempt-fetch",
            modelTurn: 1,
          },
          {
            module: "M05",
            component: "C14",
            toolInstance: "web_search",
            callId: "call-search",
            attemptId: "attempt-search",
            modelTurn: 1,
          },
        ],
      }),
    );
    const text = node.textContent ?? "";
    assertLocatable(text, {
      discovery: "call-fetch/attempt-fetch",
      issueClass: "工具执行",
      attribution: "⊕ 多个独立原因",
      recovery: "恢复：未证实全部恢复。",
      impact: "各失败需分别查看",
    });
    expect(text).toContain("call-search/attempt-search");
    expect(text).toContain("web_fetch");
    expect(text).toContain("web_search");
  });

  it("missing events are a gap, never an empty pass", async () => {
    const node = await renderReport(
      report({
        recordCompleteness: "missing-events",
        attributionStatus: "unattributed",
        headline: "记录不完整，无法确认本次执行结果。",
        impact: "缺少边界事件，不能评价迁移或质量。",
        recoveryState: "未知。",
        evidenceGaps: [
          finding("没有可关联的边界事件，不能当作执行成功", "diagnostic-gap", {
            discovery: {
              module: "M09",
              component: "C27",
              callId: "query",
              attemptId: "query",
              modelTurn: 0,
            },
          }),
        ],
        path: [
          {
            module: "M09",
            component: "C27",
            callId: "query",
            attemptId: "query",
            modelTurn: 0,
          },
        ],
      }),
    );
    assertLocatable(node.textContent ?? "", {
      discovery: "M09 → C27 → query/query",
      issueClass: "诊断缺口",
      attribution: "○ 无法归因",
      recovery: "恢复：未知。",
      impact: "缺少边界事件",
    });
    expect(node.textContent).toContain("□ 事件缺失");
  });

  it("audit persist failure is an independent health signal and a gap", async () => {
    const node = await renderReport(
      report({
        recordCompleteness: "persist-failed",
        attributionStatus: "unattributed",
        auditHealth: { persistFailed: true },
        headline: "审计写入失败，诊断可能不完整。",
        impact: "不能仅凭缺失记录推断任务成功。",
        recoveryState: "审计健康通道已标记失败。",
        evidenceGaps: [
          finding("审计写入失败", "diagnostic-gap", {
            discovery: {
              module: "M09",
              component: "C27",
              callId: "query",
              attemptId: "query",
              modelTurn: 0,
            },
          }),
        ],
      }),
    );
    const health = host.querySelector(
      '[data-testid="assistant-run-audit-health"]',
    );
    expect(health?.getAttribute("role")).toBe("status");
    expect(health?.textContent).toContain("独立健康信号");
    assertLocatable(node.textContent ?? "", {
      discovery: "M09 → C27 → query/query",
      issueClass: "诊断缺口",
      attribution: "○ 无法归因",
      recovery: "恢复：审计健康通道已标记失败。",
      impact: "不能仅凭缺失记录推断任务成功",
    });
    expect(node.textContent).toContain("⚠ 审计写入失败");
  });
});
