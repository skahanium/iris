import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("web evidence broker contract", () => {
  it("defines a unified broker and keeps low-level fetch details out of chat UI", () => {
    expect(read("src-tauri/src/ai_runtime/web_evidence_broker.rs")).toContain(
      "collect_web_evidence",
    );
    expect(read("src-tauri/src/ai_runtime/tool_catalog/web.rs")).toContain(
      "网络证据代理",
    );
    expect(read("src/components/ai/ConversationSurface.tsx")).not.toContain(
      "fetch_web_page",
    );
  });
});
