import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

function expectDisposedGuard(source: string, listenerName: string): void {
  expect(source).toContain("let disposed = false");
  expect(source).toMatch(
    new RegExp(
      `${listenerName}[\\s\\S]*then\\(\\(fn\\) => \\{[\\s\\S]*if \\(disposed\\) fn\\(\\)`,
    ),
  );
  expect(source).toMatch(
    /return \(\) => \{[\s\S]*disposed = true;[\s\S]*unlisten\?\.\(\)/,
  );
}

describe("async Tauri listener cleanup contracts", () => {
  it("guards file-change listener registration that resolves after unmount", () => {
    expectDisposedGuard(
      read("src/hooks/useCurrentFileChangeListener.ts"),
      "listenFileChanged",
    );
  });

  it("guards app-level version and classified listeners that resolve after unmount", () => {
    const source = read("src/App.impl.tsx");
    expectDisposedGuard(source, "listenVersionSaveComplete");
    expectDisposedGuard(source, "listenClassifiedFileTaken");
  });

  it("guards assistant confirmation listeners that resolve after unmount", () => {
    expectDisposedGuard(
      read("src/components/ai/hooks/useAssistantConfirmations.ts"),
      "listenForToolConfirmRequests",
    );
  });

  it("guards skill-change listeners that resolve after unmount", () => {
    expectDisposedGuard(
      read("src/components/ai/AgentStatusBadge.tsx"),
      "listenSkillsChanged",
    );
    expectDisposedGuard(
      read("src/components/ai/SkillsPanel.tsx"),
      "listenSkillsChanged",
    );
  });

  it("guards research progress listener registration that resolves after unmount", () => {
    expectDisposedGuard(
      read("src/components/ai/hooks/useResearchControl.ts"),
      "setupResearchListener",
    );
  });

  it("detaches inline AI stream listeners when the hook unmounts", () => {
    const source = read("src/hooks/useInlineAi.ts");

    expect(source).toContain("useEffect");
    expect(source).toMatch(
      /useEffect\([\s\S]*\(\) => \{[\s\S]*detachListeners\(\)/,
    );
  });
});
