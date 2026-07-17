import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  AssistantRunCapabilityDegraded,
  AssistantRunWebVerificationFailed,
} from "@/components/ai/AssistantRunCapabilityDegraded";

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
            retryable: true,
            attemptCount: 4,
            durationBucket: "budget_exhausted",
            diagnosticId: "run-web-1",
          }}
          retrying={false}
          onRetry={retry}
          onCheckConfiguration={openSettings}
        />,
      );
    });
    const alert = host.querySelector('[role="alert"]');
    expect(alert?.textContent).toContain("run-web-1");
    const buttons = host.querySelectorAll("button");
    (buttons[0] as HTMLButtonElement).click();
    expect(retry).toHaveBeenCalledOnce();
    (buttons[1] as HTMLButtonElement).click();
    expect(openSettings).toHaveBeenCalledOnce();
  });
});
