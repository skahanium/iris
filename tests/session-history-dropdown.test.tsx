import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { SessionHistoryDropdown } from "@/components/ai/SessionHistoryDropdown";
import {
  sessionClearAll,
  sessionDelete,
  sessionEvidenceList,
  sessionList,
  sessionLoad,
  sessionRename,
} from "@/lib/ipc";
import type { SessionMessageRecord, SessionSummary } from "@/types/ipc";

vi.mock("@/lib/ipc", () => ({
  sessionClearAll: vi.fn(),
  sessionDelete: vi.fn(),
  sessionEvidenceList: vi.fn(),
  sessionList: vi.fn(),
  sessionLoad: vi.fn(),
  sessionRename: vi.fn(),
}));

const mockSessionClearAll = vi.mocked(sessionClearAll);
const mockSessionDelete = vi.mocked(sessionDelete);
const mockSessionEvidenceList = vi.mocked(sessionEvidenceList);
const mockSessionList = vi.mocked(sessionList);
const mockSessionLoad = vi.mocked(sessionLoad);
const mockSessionRename = vi.mocked(sessionRename);

let root: Root | null = null;
let host: HTMLDivElement | null = null;

function renderHistory(onSelectSession = vi.fn()) {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  act(() => {
    root?.render(
      <SessionHistoryDropdown
        scene="knowledge_lookup"
        notePath={null}
        currentSessionId={null}
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
  mockSessionClearAll.mockReset();
  mockSessionDelete.mockReset();
  mockSessionEvidenceList.mockReset();
  mockSessionList.mockReset();
  mockSessionLoad.mockReset();
  mockSessionRename.mockReset();
});

describe("SessionHistoryDropdown", () => {
  it("restores session messages when the optional evidence ledger fails to load", async () => {
    const summary: SessionSummary = {
      id: 42,
      title: "Restored session",
      scene: "knowledge_lookup",
      note_path: null,
      message_count: 1,
      created_at: "2026-06-22T08:00:00.000Z",
      updated_at: "2026-06-22T08:01:00.000Z",
    };
    const record: SessionMessageRecord = {
      id: 7,
      session_id: 42,
      seq: 1,
      role: "user",
      content: "hello",
      created_at: "2026-06-22T08:01:00.000Z",
    };
    mockSessionList.mockResolvedValue([summary]);
    mockSessionLoad.mockResolvedValue([record]);
    mockSessionEvidenceList.mockRejectedValue(new Error("Database error"));
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
    ).find((element) => element.textContent?.includes("Restored session"));
    expect(sessionRow).toBeDefined();

    await act(async () => {
      sessionRow?.click();
      await Promise.resolve();
    });

    expect(onSelectSession).toHaveBeenCalledWith(
      42,
      [
        expect.objectContaining({
          role: "user",
          content: "hello",
          seq: 1,
        }),
      ],
      [],
    );
    expect(document.body.textContent).not.toContain("Database error");
  });
});
