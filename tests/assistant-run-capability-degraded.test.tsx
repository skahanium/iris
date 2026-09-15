import { readFileSync } from "node:fs";

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  AssistantRunCapabilityDegraded,
  AssistantRunWebVerificationFailed,
} from "@/components/ai/AssistantRunCapabilityDegraded";

vi.mock("@/lib/ipc", () => ({
  assistantRunDiagnose: vi.fn(),
}));

describe("AssistantRunCapabilityDegraded", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.appendChild(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("renders a nonterminal accessible Web degradation instead of a red error", () => {
    act(() => {
      root.render(
        <AssistantRunCapabilityDegraded
          degradation={{
            kind: "capability_degraded",
            capability: "web.search",
            code: "agent_run_web_provider_timeout",
            retryable: true,
            attemptCount: 2,
            message: "联网核实暂不可用，已继续生成受约束答复。",
          }}
        />,
      );
    });

    const status = host.querySelector('[role="status"]');
    expect(status?.getAttribute("aria-live")).toBe("polite");
    expect(status?.textContent).toContain("联网核实暂不可用");
    expect(status?.textContent).toContain("可稍后重试");
    expect(status?.className).not.toContain("text-destructive");

    const diagnostics = host.querySelector(
      '[data-testid="assistant-run-capability-degraded-diagnostics"]',
    );
    expect(diagnostics?.textContent).toContain(
      "agent_run_web_provider_timeout",
    );
    expect(diagnostics?.textContent).toContain("尝试次数");
    expect(diagnostics?.textContent).toContain("MCP");
  });

  it("renders a terminal diagnostic and retry action without an answer", () => {
    const retry = vi.fn();
    const openSettings = vi.fn();
    act(() => {
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
          onRetry={retry}
          onCheckConfiguration={openSettings}
          session={{ domain: "normal", sessionKey: "session-web" }}
        />,
      );
    });
    const alert = host.querySelector('[role="alert"]');
    expect(alert?.textContent).toContain("查看诊断");
    expect(alert?.textContent).not.toContain("诊断编号");
    const buttons = [...host.querySelectorAll("button")];
    buttons
      .find((button) => button.textContent?.includes("重试联网核实"))
      ?.click();
    expect(retry).toHaveBeenCalledOnce();
    buttons
      .find((button) => button.textContent?.includes("检查联网配置"))
      ?.click();
    expect(openSettings).toHaveBeenCalledOnce();
  });

  it("explains a response safety-cap failure without provider output", () => {
    act(() => {
      root.render(
        <AssistantRunWebVerificationFailed
          failure={{
            kind: "web_verification_failed",
            code: "agent_run_web_evidence_invalid",
            failureReason: "provider_output_too_large",
            retryable: false,
            attemptCount: 1,
            durationBucket: "3s_to_15s",
            diagnosticId: "run-output-limit",
          }}
          retrying={false}
          onRetry={vi.fn()}
        />,
      );
    });

    expect(host.textContent).toContain("超过安全上限");
    expect(host.textContent).not.toContain("private provider output");
  });

  it("explains a local-material query block without exposing the material", () => {
    act(() => {
      root.render(
        <AssistantRunWebVerificationFailed
          failure={{
            kind: "web_verification_failed",
            code: "agent_run_web_evidence_invalid",
            failureReason: "local_material_query_blocked",
            retryable: false,
            attemptCount: 1,
            durationBucket: "not_started",
            diagnosticId: "run-private-query",
          }}
          retrying={false}
          onRetry={vi.fn()}
        />,
      );
    });

    expect(host.textContent).toContain("保护你授权的材料");
    expect(host.textContent).toContain("公开关键词");
    expect(host.textContent).not.toContain("Iris Pilot");
  });

  it("renders the precise non-retryable API Key remediation message", () => {
    act(() => {
      root.render(
        <AssistantRunCapabilityDegraded
          degradation={{
            kind: "capability_degraded",
            capability: "web.search",
            code: "agent_run_web_provider_auth_failed",
            retryable: false,
            attemptCount: 1,
            message:
              "联网 API Key 无效，请重新输入原始 Key；已继续生成不依赖联网证据的受约束答复。",
          }}
        />,
      );
    });

    expect(host.textContent).toContain("联网 API Key 无效，请重新输入原始 Key");
    expect(host.textContent).not.toContain("可稍后重试");
  });

  it("explains that diagnosis needs the current session", () => {
    act(() => {
      root.render(
        <AssistantRunCapabilityDegraded
          degradation={{
            kind: "capability_degraded",
            capability: "web.search",
            code: "agent_run_web_provider_timeout",
            retryable: true,
            attemptCount: 1,
            message: "联网核实暂不可用，已继续生成受约束答复。",
          }}
        />,
      );
    });
    expect(host.textContent).toContain("诊断入口需要当前会话");
  });

  it("is wired into the production assistant panel event projection", () => {
    const source = readFileSync(
      "src/components/ai/UnifiedAssistantPanel.impl.tsx",
      "utf8",
    );

    expect(source).toContain("AssistantRunCapabilityDegraded");
    expect(source).toContain("eventState?.capabilityDegradation");
    expect(source).toContain("<AssistantRunCapabilityDegraded");
    expect(source).toContain("AssistantRunWebVerificationFailed");
    expect(source).toContain("session={runSession}");
  });
});
