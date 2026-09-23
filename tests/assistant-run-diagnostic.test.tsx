import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AssistantRunDiagnosticEntry } from "@/components/ai/AssistantRunDiagnostic";
import { AiMessageList } from "@/components/ai/AiMessageList";
import { AssistantRunWebVerificationFailed } from "@/components/ai/AssistantRunCapabilityDegraded";
import { assistantRunDiagnose } from "@/lib/ipc";
import type { DiagnosticReport } from "@/types/ai";

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getTotalSize: () => count * 112,
    getVirtualItems: () =>
      Array.from({ length: count }, (_, index) => ({
        index,
        key: `row-${index}`,
        start: index * 112,
      })),
    measureElement: vi.fn(),
  }),
}));

vi.mock("@/lib/ipc", () => ({
  assistantRunDiagnose: vi.fn(),
}));

const diagnose = vi.mocked(assistantRunDiagnose);

const session = { domain: "normal" as const, sessionKey: "session-1" };

function sampleReport(
  overrides: Partial<DiagnosticReport> = {},
): DiagnosticReport {
  return {
    schemaVersion: 1,
    runId: "run-1",
    inputRevision: "rev-1",
    parentRunId: null,
    childRunId: null,
    recordCompleteness: "complete",
    attributionStatus: "confirmed",
    auditHealth: { persistFailed: false },
    headline: "工具 web_fetch 执行失败。",
    impact: "本次工具调用未完成。",
    recoveryState: "已记录修复尝试。",
    knownFacts: [],
    directFailures: [
      {
        statement: "工具 web_fetch 执行失败",
        discovery: {
          module: "M05",
          component: "C14",
          toolInstance: "web_fetch",
          callId: "call-1",
          attemptId: "attempt-1",
          modelTurn: 1,
        },
        issueClass: "tool-execution",
        confirmedSource: "tool-execution",
      },
    ],
    recoveryResults: [
      {
        statement: "恢复耗尽，这是恢复结果而不是根因",
        discovery: {
          module: "M05",
          component: "C14",
          callId: "call-1",
          attemptId: "attempt-1",
          modelTurn: 1,
        },
        issueClass: "internal-contract",
      },
    ],
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

describe("AssistantRunDiagnosticEntry", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    diagnose.mockReset();
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("loads the current Run on click without asking for a copied ID", async () => {
    diagnose.mockResolvedValue(sampleReport());
    await act(async () => {
      root.render(
        <AssistantRunDiagnosticEntry session={session} runId="run-1" />,
      );
    });
    const button = host.querySelector(
      '[data-testid="assistant-run-diagnose-open"]',
    ) as HTMLButtonElement;
    expect(button.textContent).toBe("查看诊断");
    await act(async () => {
      button.click();
    });
    expect(diagnose).toHaveBeenCalledWith({ session, runId: "run-1" });
    const report = host.querySelector(
      '[data-testid="assistant-run-diagnostic-report"]',
    );
    expect(report?.textContent).toContain("工具 web_fetch 执行失败");
    expect(report?.textContent).toContain("● 已证实");
    expect(report?.textContent).toContain("M05 → C14 → call-1/attempt-1");
    expect(report?.textContent).toContain("恢复耗尽");
    expect(report?.textContent).not.toContain("未发现问题");
    expect(report?.textContent).not.toContain("未发现可证实的故障");
  });

  it("shows query failure instead of an empty success", async () => {
    diagnose.mockRejectedValue({ code: "agent_run_persistence_failed" });
    await act(async () => {
      root.render(
        <AssistantRunDiagnosticEntry session={session} runId="run-1" />,
      );
    });
    await act(async () => {
      (
        host.querySelector(
          '[data-testid="assistant-run-diagnose-open"]',
        ) as HTMLButtonElement
      ).click();
    });
    const failed = host.querySelector(
      '[data-testid="assistant-run-diagnose-query-failed"]',
    );
    expect(failed?.getAttribute("role")).toBe("alert");
    expect(failed?.textContent).toContain("诊断查询失败");
    expect(failed?.textContent).toContain("空成功");
    expect(failed?.textContent).not.toContain("未发现问题");
    expect(
      host.querySelector('[data-testid="assistant-run-diagnostic-report"]'),
    ).toBeNull();
  });

  it("keeps persist-failed health independent of the finding lists", async () => {
    diagnose.mockResolvedValue(
      sampleReport({
        recordCompleteness: "persist-failed",
        attributionStatus: "unattributed",
        auditHealth: { persistFailed: true },
        headline: "审计写入失败，诊断可能不完整。",
        knownFacts: [],
        directFailures: [],
        recoveryResults: [],
      }),
    );
    await act(async () => {
      root.render(
        <AssistantRunDiagnosticEntry session={session} runId="run-1" />,
      );
    });
    await act(async () => {
      (
        host.querySelector(
          '[data-testid="assistant-run-diagnose-open"]',
        ) as HTMLButtonElement
      ).click();
    });
    const health = host.querySelector(
      '[data-testid="assistant-run-audit-health"]',
    );
    expect(health?.getAttribute("role")).toBe("status");
    expect(health?.textContent).toContain("独立健康信号");
    expect(host.textContent).toContain("⚠ 审计写入失败");
  });
});

describe("diagnostic entry from conversation and failure banners", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    diagnose.mockReset();
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("offers diagnosis from an assistant answer that already has a runId", async () => {
    await act(async () => {
      root.render(
        <AiMessageList
          messages={[
            {
              role: "assistant",
              content: "已完成。",
              runId: "run-answer",
            },
          ]}
          streaming={false}
          session={session}
        />,
      );
    });
    const button = host.querySelector(
      '[data-testid="assistant-run-diagnose-open"]',
    ) as HTMLButtonElement;
    expect(button).not.toBeNull();
    expect(button.closest(".ai-message-bubble")).toBeNull();
    diagnose.mockResolvedValue(sampleReport({ runId: "run-answer" }));
    await act(async () => {
      button.click();
    });
    expect(diagnose).toHaveBeenCalledWith({
      session,
      runId: "run-answer",
    });
  });

  it("uses the web-verification diagnosticId as the current Run", async () => {
    diagnose.mockResolvedValue(sampleReport({ runId: "run-web-1" }));
    await act(async () => {
      root.render(
        <AssistantRunWebVerificationFailed
          failure={{
            kind: "web_verification_failed",
            code: "agent_run_web_provider_timeout",
            failureReason: "provider_timeout",
            retryable: true,
            attemptCount: 4,
            durationBucket: "budget_exhausted",
            diagnosticId: "run-web-1",
          }}
          retrying={false}
          onRetry={vi.fn()}
          session={session}
        />,
      );
    });
    expect(host.textContent).toContain("查看诊断");
    expect(host.textContent).not.toContain("诊断编号");
    await act(async () => {
      (
        host.querySelector(
          '[data-testid="assistant-run-diagnose-open"]',
        ) as HTMLButtonElement
      ).click();
    });
    expect(diagnose).toHaveBeenCalledWith({
      session,
      runId: "run-web-1",
    });
  });
});
