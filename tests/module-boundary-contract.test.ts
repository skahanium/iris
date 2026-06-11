import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function lineCount(path: string): number {
  return readFileSync(path, "utf8").split(/\r?\n/).length;
}

describe("module boundary contract", () => {
  it("keeps frontend shell and assistant entry modules thin", () => {
    expect(
      lineCount("src/components/ai/UnifiedAssistantPanel.tsx"),
    ).toBeLessThanOrEqual(520);
    expect(lineCount("src/App.tsx")).toBeLessThanOrEqual(820);
  });

  it("keeps Rust AI runtime facade modules thin", () => {
    for (const path of [
      "src-tauri/src/ai_runtime/skills.rs",
      "src-tauri/src/ai_runtime/model_gateway.rs",
      "src-tauri/src/ai_runtime/tool_dispatch.rs",
      "src-tauri/src/ai_runtime/tool_catalog.rs",
      "src-tauri/src/ai_runtime/retrieval_broker.rs",
    ]) {
      expect(lineCount(path), `${path} is still too large`).toBeLessThanOrEqual(
        700,
      );
    }
  });
});
