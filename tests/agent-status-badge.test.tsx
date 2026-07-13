import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { AgentStatusBadge } from "@/components/ai/AgentStatusBadge";

describe("AgentStatusBadge", () => {
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

  it("shows the unified Run state and selected web provider without a scene", () => {
    act(() => {
      root.render(
        <AgentStatusBadge
          webSearchEnabled
          webSearchProviderName="AnySearch"
          runState="running"
        />,
      );
    });

    const trigger = document.querySelector<HTMLButtonElement>(
      '[data-testid="agent-status-trigger"]',
    );
    expect(trigger?.textContent).toContain("正在回答");
    expect(trigger?.title).toContain("AnySearch");
  });

  it("shows a closed web search state without legacy task statistics", () => {
    act(() => {
      root.render(
        <AgentStatusBadge webSearchEnabled={false} runState="idle" />,
      );
    });

    const trigger = document.querySelector<HTMLButtonElement>(
      '[data-testid="agent-status-trigger"]',
    );
    expect(trigger?.textContent).toContain("准备就绪");
    expect(trigger?.title).toContain("联网：已关闭");
  });
});
