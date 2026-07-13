import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { SessionHistoryDropdown } from "@/components/ai/SessionHistoryDropdown";
import {
  assistantSessionDelete,
  assistantSessionList,
  assistantSessionLoad,
  assistantSessionRename,
  assistantRunGet,
} from "@/lib/ipc";
import type {
  AssistantSessionMessage,
  AssistantSessionSummary,
} from "@/types/ai";

vi.mock("@/lib/ipc", () => ({
  assistantSessionDelete: vi.fn(),
  assistantSessionList: vi.fn(),
  assistantSessionLoad: vi.fn(),
  assistantSessionRename: vi.fn(),
  assistantRunGet: vi.fn(),
}));

const mockAssistantSessionDelete = vi.mocked(assistantSessionDelete);
const mockAssistantSessionList = vi.mocked(assistantSessionList);
const mockAssistantSessionLoad = vi.mocked(assistantSessionLoad);
const mockAssistantSessionRename = vi.mocked(assistantSessionRename);
const mockAssistantRunGet = vi.mocked(assistantRunGet);

let root: Root | null = null;
let host: HTMLDivElement | null = null;

function renderHistory(onSelectSession = vi.fn()) {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  act(() => {
    root?.render(
      <SessionHistoryDropdown
        domain="normal"
        onSelectSession={onSelectSession}
      />,
    );
  });
  return onSelectSession;
}

async function flushPromises() {
  await act(async () => {
    await Promise.resolve();
  });
}

afterEach(() => {
  if (root) {
    act(() => root?.unmount());
  }
  host?.remove();
  root = null;
  host = null;
  for (const mock of [
    mockAssistantSessionDelete,
    mockAssistantSessionList,
    mockAssistantSessionLoad,
    mockAssistantSessionRename,
    mockAssistantRunGet,
  ]) {
    mock.mockReset();
  }
});

describe("SessionHistoryDropdown", () => {
  it("loads and restores opaque domain-safe conversations", async () => {
    const summary: AssistantSessionSummary = {
      session: { domain: "normal", sessionKey: "conversation-42" },
      title: "Restored conversation",
      messageCount: 1,
      createdAt: "2026-06-22T08:00:00.000Z",
      updatedAt: "2026-06-22T08:01:00.000Z",
    };
    const message: AssistantSessionMessage = {
      seq: 1,
      role: "user",
      content: "hello",
      explicitReferences: [],
      createdAt: "2026-06-22T08:01:00.000Z",
    };
    mockAssistantSessionList.mockResolvedValue([summary]);
    mockAssistantSessionLoad.mockResolvedValue([message]);
    mockAssistantRunGet.mockResolvedValue(null);
    const onSelectSession = renderHistory();

    act(() => {
      document
        .querySelector<HTMLButtonElement>(
          '[data-testid="session-history-trigger"]',
        )
        ?.click();
    });
    await flushPromises();

    const sessionRow = Array.from(
      document.querySelectorAll<HTMLElement>('[role="button"]'),
    ).find((element) => element.textContent?.includes("Restored conversation"));
    expect(sessionRow).toBeDefined();

    await act(async () => {
      sessionRow?.click();
      await Promise.resolve();
    });

    expect(mockAssistantSessionList).toHaveBeenCalledWith({
      domain: "normal",
      limit: 40,
    });
    expect(mockAssistantSessionLoad).toHaveBeenCalledWith({
      session: summary.session,
    });
    expect(mockAssistantRunGet).toHaveBeenCalledWith({
      session: summary.session,
    });
    expect(onSelectSession).toHaveBeenCalledWith(
      summary.session,
      [{ role: "user", content: "hello", seq: 1 }],
      null,
    );
  });
});
