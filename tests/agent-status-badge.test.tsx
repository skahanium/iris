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

  it("shows the selected web search provider instead of a generic enabled state", async () => {
    await act(async () => {
      root.render(
        <AgentStatusBadge
          webSearchEnabled
          webSearchProviderName="AnySearch"
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
    expect(document.body.textContent).toContain("AnySearch");
    expect(document.body.textContent).not.toContain("已开启");
    expect(document.body.textContent).not.toContain("MCP");
    expect(document.body.textContent).not.toContain("DDG");
    expect(document.body.textContent).not.toContain("次");
  });

  it("shows a closed web search state without provider statistics", async () => {
    await act(async () => {
      root.render(
        <AgentStatusBadge
          webSearchEnabled={false}
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

    expect(document.body.textContent).toContain("未开启");
    expect(document.body.textContent).not.toContain("MCP 0");
    expect(document.body.textContent).not.toContain("DDG");
  });
});
