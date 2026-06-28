import { describe, expect, it } from "vitest";

import {
  ARTIFACT_TAB_STORAGE_KEY,
  buildArtifactTab,
  loadArtifactTabsSnapshot,
  saveArtifactTabsSnapshot,
} from "@/lib/assistant-artifact-tabs";
import type { AssistantArtifactDraft } from "@/types/assistant-artifact";

function draft(id: number): AssistantArtifactDraft {
  return {
    kind: "task_process",
    title: `过程详情 ${id}`,
    sourceRequestId: `req-${id}`,
    payload: {
      visible: `摘要 ${id}`,
      request_id: `req-${id}`,
      checkpoint_json: "raw",
      noteContent: "用户笔记全文",
      apiKey: "secret",
      nested: { token: "secret-token", keep: "ok" },
    },
  };
}

describe("assistant artifact tabs", () => {
  it("builds readonly artifact tabs with sanitized payloads", () => {
    const tab = buildArtifactTab(draft(1), "2026-06-20T00:00:00.000Z");

    expect(tab.id).toBe("artifact:task_process:req-1");
    expect(tab.readonly).toBe(true);
    expect(JSON.stringify(tab)).toContain("摘要 1");
    expect(JSON.stringify(tab)).not.toContain("checkpoint_json");
    expect(JSON.stringify(tab)).not.toContain("用户笔记全文");
    expect(JSON.stringify(tab)).not.toContain("secret-token");
  });

  it("persists only the latest ten sanitized artifact tabs", () => {
    const memory = new Map<string, string>();
    const storage: Storage = {
      get length() {
        return memory.size;
      },
      clear: () => memory.clear(),
      getItem: (key) => memory.get(key) ?? null,
      key: (index) => Array.from(memory.keys())[index] ?? null,
      removeItem: (key) => memory.delete(key),
      setItem: (key, value) => {
        memory.set(key, value);
      },
    };
    const tabs = Array.from({ length: 12 }, (_, index) =>
      buildArtifactTab(draft(index + 1), `2026-06-20T00:00:${index}.000Z`),
    );

    saveArtifactTabsSnapshot(storage, tabs);
    const restored = loadArtifactTabsSnapshot(storage);

    expect(restored).toHaveLength(10);
    expect(restored[0]?.sourceRequestId).toBe("req-3");
    expect(restored[9]?.sourceRequestId).toBe("req-12");
    expect(storage.getItem(ARTIFACT_TAB_STORAGE_KEY)).not.toContain(
      "noteContent",
    );
  });

  it("does not persist temporary evidence detail tabs", () => {
    const memory = new Map<string, string>();
    const storage: Storage = {
      get length() {
        return memory.size;
      },
      clear: () => memory.clear(),
      getItem: (key) => memory.get(key) ?? null,
      key: (index) => Array.from(memory.keys())[index] ?? null,
      removeItem: (key) => memory.delete(key),
      setItem: (key, value) => {
        memory.set(key, value);
      },
    };
    const persistent = buildArtifactTab(draft(1), "2026-06-20T00:00:00.000Z");
    const temporary = buildArtifactTab(
      {
        kind: "session_evidence_detail",
        title: "Evidence Detail",
        sourceRequestId: "session-1",
        payload: { sessionId: 1 },
        persistent: false,
      },
      "2026-06-20T00:00:01.000Z",
    );

    saveArtifactTabsSnapshot(storage, [persistent, temporary]);

    expect(loadArtifactTabsSnapshot(storage)).toEqual([persistent]);
    expect(storage.getItem(ARTIFACT_TAB_STORAGE_KEY)).not.toContain(
      "session_evidence_detail",
    );
  });

  it("drops persisted tabs with legacy artifact kinds", () => {
    const memory = new Map<string, string>();
    const storage: Storage = {
      get length() {
        return memory.size;
      },
      clear: () => memory.clear(),
      getItem: (key) => memory.get(key) ?? null,
      key: (index) => Array.from(memory.keys())[index] ?? null,
      removeItem: (key) => memory.delete(key),
      setItem: (key, value) => {
        memory.set(key, value);
      },
    };
    const valid = buildArtifactTab(draft(1), "2026-06-20T00:00:00.000Z");
    storage.setItem(
      ARTIFACT_TAB_STORAGE_KEY,
      JSON.stringify([
        valid,
        {
          ...valid,
          id: "artifact:process:req-old",
          kind: ["pro", "cess"].join(""),
          title: "旧过程详情",
        },
      ]),
    );

    expect(loadArtifactTabsSnapshot(storage)).toEqual([valid]);
  });
});
