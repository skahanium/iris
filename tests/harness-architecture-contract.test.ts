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

const removedDdg = ["duck", "duck", "go"].join("");
const removedVendor = ["mini", "max"].join("");
const removedVendorCredential = ["MINI", "MAX", "_CREDENTIAL_SERVICE"].join("");

describe("AI harness architecture contracts", () => {
  it("keeps MCP tools/list diagnostic and out of the model tool surface", () => {
    const host = read("src-tauri/src/ai_runtime/mcp_host_runtime.rs");
    const diagnostics = read("src-tauri/src/commands/ai_commands.rs");
    const policy = read("src-tauri/src/ai_runtime/tool_policy.rs");
    const resolver = read("src-tauri/src/ai_runtime/capability_resolver.rs");
    const executor = read("src-tauri/src/ai_runtime/tool_executor.rs");

    expect(host).toContain("discover_provider_tools");
    expect(host).toContain("discover_provider_tools_without_recording");
    expect(host).toContain("call_provider_tool");
    expect(diagnostics).toContain("MCP 服务已响应 tools/list");

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

  it("keeps vendor search out of web evidence routing surfaces", () => {
    const broker = read("src-tauri/src/ai_runtime/web_evidence_broker.rs");
    const management = read(
      "src/components/settings/ManagementCenterPanel.tsx",
    );
    const connectivity = read(
      "src/components/layout/ConnectivityIndicators.tsx",
    );

    expect(broker).toContain("SearchProviderCandidate::Mcp");
    expect(broker).not.toContain("SearchProviderCandidate::Native");
    expect(broker.toLowerCase()).not.toContain(removedDdg);
    expect(broker.toLowerCase()).not.toContain(removedVendor);
    const candidateStart = broker.indexOf("fn search_provider_candidates");
    const candidateEnd = broker.indexOf(
      "async fn collect_search_provider_fetches",
      candidateStart,
    );
    const candidateBody = broker.slice(candidateStart, candidateEnd);

    expect(candidateBody.toLowerCase()).not.toContain(removedVendor);
    expect(candidateBody).not.toContain(removedVendorCredential);
    expect(management.toLowerCase()).not.toContain(
      `${removedVendor}searchsection`,
    );
    expect(connectivity.toLowerCase()).not.toContain(removedVendor);
  });
});

describe("Run runtime module ownership", () => {
  it("does not retain the legacy ai_harness module or helper namespace", () => {
    const runtime = read("src-tauri/src/ai_runtime/mod.rs");
    const lib = read("src-tauri/src/lib.rs");

    expect(runtime).not.toContain("ai_harness");
    expect(runtime).not.toContain("harness_support");
    expect(lib).not.toContain("ai_harness");
  });
});
