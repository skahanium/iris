import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AgentStatusBadge } from "@/components/ai/AgentStatusBadge";

vi.mock("@/lib/ipc", () => ({
  listenSkillsChanged: vi.fn(async () => () => {}),
  skillsList: vi.fn(async () => []),
}));

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

  it("shows successful MCP and DDG web search request counts", async () => {
    await act(async () => {
      root.render(
        <AgentStatusBadge
          webSearchEnabled
          webSearchUsage={{
            successfulSearchRequests: { mcp: 1, duckduckgo: 1 },
          }}
          scene="knowledge_lookup"
          taskStatus="completed"
        />,
      );
    });

    await act(async () => {
      document
        .querySelector<HTMLButtonElement>(
          '[data-testid="agent-status-trigger"]',
        )
        ?.click();
    });

    expect(document.body.textContent).toContain("联网搜索");
    expect(document.body.textContent).toContain("已开启 · MCP 1 次 · DDG 1 次");
  });

  it("does not show success counts when no provider returned valid results", async () => {
    await act(async () => {
      root.render(
        <AgentStatusBadge
          webSearchEnabled
          webSearchUsage={{
            successfulSearchRequests: { mcp: 0, duckduckgo: 0 },
          }}
          scene="knowledge_lookup"
          taskStatus="completed"
        />,
      );
    });

    await act(async () => {
      document
        .querySelector<HTMLButtonElement>(
          '[data-testid="agent-status-trigger"]',
        )
        ?.click();
    });

    expect(document.body.textContent).toContain("已开启 · 暂无成功结果");
    expect(document.body.textContent).not.toContain("MCP 0 次");
  });
});
