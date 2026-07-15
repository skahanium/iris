import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { AssistantRunCapabilityDegraded } from "@/components/ai/AssistantRunCapabilityDegraded";

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
});
