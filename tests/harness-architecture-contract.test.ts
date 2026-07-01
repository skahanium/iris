import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

function readAll(root: string): string {
  const chunks: string[] = [];
  const visit = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) {
        visit(path);
        continue;
      }
      if (/\.(rs|sql|ts|tsx|md)$/.test(path)) {
        chunks.push(read(path));
      }
    }
  };
  visit(root);
  return chunks.join("\n");
}

describe("AI harness architecture contracts", () => {
  it("keeps MCP tools/list diagnostic and out of the model tool surface", () => {
    const host = read("src-tauri/src/ai_runtime/mcp_host_runtime.rs");
    const policy = read("src-tauri/src/ai_runtime/tool_policy.rs");
    const resolver = read("src-tauri/src/ai_runtime/capability_resolver.rs");
    const executor = read("src-tauri/src/ai_runtime/tool_executor.rs");

    expect(host).toContain("tools/list");
    expect(host).toContain("call_provider_tool");

    for (const rawTool of [
      "mcp.raw_tool_call",
      "mcp_runtime_tools_list",
      "mcp_runtime_capability_call",
      "mcp_runtime_profile_upsert",
    ]) {
      expect(policy).not.toContain(`name: "${rawTool}"`);
      expect(executor).not.toContain(`name: "${rawTool}"`);
      expect(resolver).not.toContain(`"${rawTool}" =>`);
    }

    expect(policy).toContain(
      'const META_SKILL_TOOLS: &[&str] = &["skills_list"]',
    );
    expect(policy).not.toContain("tools/list result");
  });

  it("does not define cross-session prompt response caching", () => {
    const migrations = readAll("src-tauri/migrations");
    const rustSources = readAll("src-tauri/src");
    const frontendSources = readAll("src");

    expect(migrations).not.toMatch(/prompt_response_cache|llm_response_cache/);
    expect(rustSources).not.toMatch(
      /save_full_prompt|cachePromptResponse|prompt_response_cache|llm_response_cache/,
    );
    expect(frontendSources).not.toMatch(
      /saveFullPrompt|cachePromptResponse|prompt_response_cache|llm_response_cache/,
    );
  });

  it("keeps MiniMax out of web evidence routing surfaces", () => {
    const broker = read("src-tauri/src/ai_runtime/web_evidence_broker.rs");
    const management = read(
      "src/components/settings/ManagementCenterPanel.tsx",
    );
    const connectivity = read(
      "src/components/layout/ConnectivityIndicators.tsx",
    );

    expect(broker).toContain("SearchProviderCandidate::Mcp");
    expect(broker).toContain("WebSearchEffectiveBackend::Duckduckgo");
    const candidateStart = broker.indexOf("fn search_provider_candidates");
    const candidateEnd = broker.indexOf(
      "async fn collect_search_provider_fetches",
      candidateStart,
    );
    const candidateBody = broker.slice(candidateStart, candidateEnd);

    expect(candidateBody).not.toContain("WebSearchEffectiveBackend::Minimax");
    expect(candidateBody).not.toContain("MINIMAX_CREDENTIAL_SERVICE");
    expect(management).not.toContain("MinimaxSearchSection");
    expect(connectivity).not.toContain('effectiveBackend === "minimax"');
  });
});
