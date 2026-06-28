import { beforeEach, describe, expect, it } from "vitest";

import {
  loadWorkspaceSessionSnapshot,
  saveWorkspaceSessionSnapshot,
} from "../src/lib/workspace-session-snapshot";

beforeEach(() => {
  localStorage.clear();
});

describe("workspace session snapshot", () => {
  it("persists paths and tab metadata without note content or editor html", () => {
    saveWorkspaceSessionSnapshot("vault-a", {
      activePath: "notes/a.md",
      openNotes: [
        {
          path: "notes/a.md",
          title: "A",
          isLocked: false,
          lastActiveAt: 10,
        },
      ],
    });

    const raw = localStorage.getItem("iris.workspace-session.v1:vault-a") ?? "";
    expect(raw).toContain("notes/a.md");
    expect(raw).not.toContain("markdown");
    expect(raw).not.toContain("editorHtml");
    expect(raw).not.toContain("content");

    expect(loadWorkspaceSessionSnapshot("vault-a")).toEqual({
      version: 1,
      savedAt: expect.any(Number),
      activePath: "notes/a.md",
      openNotes: [
        {
          path: "notes/a.md",
          title: "A",
          isLocked: false,
          lastActiveAt: 10,
        },
      ],
    });
  });

  it("drops malformed stored snapshots instead of throwing during startup", () => {
    localStorage.setItem("iris.workspace-session.v1:vault-a", "{broken");

    expect(loadWorkspaceSessionSnapshot("vault-a")).toBeNull();
  });
});
