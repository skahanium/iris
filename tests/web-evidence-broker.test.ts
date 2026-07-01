import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("web evidence broker contract", () => {
  it("defines a unified broker and keeps low-level fetch details out of chat UI", () => {
    const broker = read("src-tauri/src/ai_runtime/web_evidence_broker.rs");

    expect(broker).toContain("collect_web_evidence");
    expect(broker).toContain("list_enabled_web_provider_mappings");
    expect(broker).not.toContain("fetch_search_context_for_db");
    expect(read("src-tauri/src/ai_runtime/tool_catalog/web.rs")).toContain(
      "网络证据代理",
    );
    expect(read("src/components/ai/ConversationSurface.tsx")).not.toContain(
      "fetch_web_page",
    );
  });

  it("does not use MiniMax as a web evidence backend", () => {
    const broker = read("src-tauri/src/ai_runtime/web_evidence_broker.rs");
    const candidateBody =
      broker
        .split("fn search_provider_candidates")[1]
        ?.split("async fn collect_search_provider_fetches")[0] ?? "";

    expect(candidateBody).toContain("SearchProviderCandidate::Mcp");
    expect(candidateBody).toContain("WebSearchEffectiveBackend::Duckduckgo");
    expect(candidateBody).not.toContain("WebSearchEffectiveBackend::Minimax");
    expect(candidateBody).not.toContain("MINIMAX_CREDENTIAL_SERVICE");
  });
});
