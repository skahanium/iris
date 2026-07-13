import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

const root = resolve(".");

function read(relativePath: string): string {
  return readFileSync(resolve(root, relativePath), "utf8");
}

describe("Run-only artifact cleanup contract", () => {
  it("removes the legacy artifact and session-evidence UI chain", () => {
    expect(existsSync(resolve(root, "src/types/assistant-artifact.ts"))).toBe(
      false,
    );
    expect(existsSync(resolve(root, "src/hooks/useArtifactTabs.ts"))).toBe(
      false,
    );
    expect(
      existsSync(resolve(root, "src/lib/assistant-artifact-tabs.ts")),
    ).toBe(false);
    expect(
      existsSync(resolve(root, "src/components/ai/EvidenceDetailArtifact.tsx")),
    ).toBe(false);
    expect(
      existsSync(
        resolve(root, "src/components/layout/ArtifactWorkspaceView.tsx"),
      ),
    ).toBe(false);

    const app = read("src/App.impl.tsx");
    const workspace = read("src/hooks/useWorkspaceTabRouting.ts");
    const ipcTypes = read("src/types/ipc.ts");

    expect(app).not.toContain("useArtifactTabs");
    expect(workspace).not.toContain("ArtifactTab");
    expect(ipcTypes).not.toContain("SessionEvidenceDetailRecord");
    expect(ipcTypes).not.toContain("AgentTaskDto");
  });
});
