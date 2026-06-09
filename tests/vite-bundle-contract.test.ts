import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Vite bundle contract", () => {
  it("keeps the default chunk warning limit and splits major vendors", () => {
    const source = read("vite.config.ts");

    expect(source).toContain("chunkSizeWarningLimit: 500");
    for (const chunk of [
      "react-vendor",
      "tauri-vendor",
      "ui-vendor",
      "markdown-vendor",
      "icons-vendor",
      "tiptap",
      "prosemirror",
    ]) {
      expect(source).toContain(chunk);
    }
  });

  it("does not dynamically import the Tauri event API", () => {
    const eventListenerCallSites = [
      "src/App.tsx",
      "src/components/ai/AgentStatusBadge.tsx",
      "src/components/ai/SkillsPanel.tsx",
      "src/components/ai/UnifiedAssistantPanel.tsx",
      "src/lib/ipc.ts",
    ];
    const offenders = eventListenerCallSites.filter((path) =>
      read(path).includes('import("@tauri-apps/api/event")'),
    );

    expect(offenders).toEqual([]);
  });
});
