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
      content: "hello Guide",
      explicitReferences: [],
      contextScope: {
        paths: [],
        pathPrefixes: [],
        requiredTags: [],
      },
      displayMentions: [
        {
          kind: "file",
          value: "notes/Guide.md",
          label: "Guide",
          range: { from: 6, to: 11 },
        },
      ],
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
      [
        {
          role: "user",
          content: "hello Guide",
          displayMentions: message.displayMentions,
          seq: 1,
          created_at: "2026-06-22T08:01:00.000Z",
        },
      ],
      null,
    );
  });

  it("restores persisted process events only onto their assistant message", async () => {
    const summary: AssistantSessionSummary = {
      session: { domain: "normal", sessionKey: "conversation-process" },
      title: "Process history",
      messageCount: 2,
      createdAt: "2026-07-22T08:00:00.000Z",
      updatedAt: "2026-07-22T08:01:00.000Z",
    };
    const messages: AssistantSessionMessage[] = [
      {
        seq: 1,
        role: "user",
        content: "核验资料",
        turnId: "turn-process-1",
        explicitReferences: [],
        contextScope: [],
        displayMentions: [],
        createdAt: "2026-07-22T08:00:00.000Z",
      },
      {
        seq: 2,
        role: "assistant",
        content: "最终答复",
        runId: "run-process-1",
        turnId: "turn-process-1",
        processEvents: [
          {
            runId: "run-process-1",
            seq: 2,
            stateVersion: 1,
            timestamp: "2026-07-22T08:00:01.000Z",
            type: "stage_changed",
            payload: {
              kind: "stage_changed",
              state: "running",
              stage: "正在核验资料",
            },
          },
        ],
        explicitReferences: [],
        contextScope: [],
        displayMentions: [],
        createdAt: "2026-07-22T08:01:00.000Z",
      },
    ];
    mockAssistantSessionList.mockResolvedValue([summary]);
    mockAssistantSessionLoad.mockResolvedValue(messages);
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
    ).find((element) => element.textContent?.includes("Process history"));

    await act(async () => {
      sessionRow?.click();
      await Promise.resolve();
    });

    expect(onSelectSession).toHaveBeenCalledWith(
      summary.session,
      [
        {
          role: "user",
          content: "核验资料",
          turnId: "turn-process-1",
          seq: 1,
          created_at: "2026-07-22T08:00:00.000Z",
        },
        {
          role: "assistant",
          content: "最终答复",
          runId: "run-process-1",
          turnId: "turn-process-1",
          processItems: [
            {
              id: "stage:2",
              kind: "stage",
              label: "正在核验资料",
              status: "completed",
              createdAt: Date.parse("2026-07-22T08:00:01.000Z"),
            },
          ],
          seq: 2,
          created_at: "2026-07-22T08:01:00.000Z",
        },
      ],
      null,
    );
  });
});
