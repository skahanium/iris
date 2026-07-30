import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("UnifiedAssistantPanel startup performance", () => {
  it("defers optional MCP binding discovery until after the first paint", () => {
    const src = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");

    expect(src).toContain("scheduleExternalBindingsLoad");
    expect(src).toContain("requestAnimationFrame");
    expect(src).toContain("mcpCapabilityBindingsList");
    expect(src).toContain("cancelAnimationFrame");
  });
});
