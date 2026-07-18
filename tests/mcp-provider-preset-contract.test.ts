import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { MCP_PROVIDER_PRESETS } from "@/components/ai/skills/mcpProviderPresets";
import { presetDeclaredResultLimitTargets } from "@/components/ai/skills/mcpSearchMappingHeal";
import builtinLlmProviders from "../config/llm-builtin-providers.json";
import mcpOptionalCredentials from "../config/mcp-optional-credential-services.json";
import mcpSearchResultLimitManifest from "../config/mcp-search-result-limit-manifest.json";

const EXPECTED_PRESET_IDS = [
  "anysearch",
  "jina",
  "firecrawl",
  "brave",
  "searxng",
] as const;

describe("provider manifest contract", () => {
  it("keeps the supported MCP preset catalog aligned with the contract list", () => {
    expect(MCP_PROVIDER_PRESETS.map((preset) => preset.id)).toEqual([
      ...EXPECTED_PRESET_IDS,
    ]);
    expect(EXPECTED_PRESET_IDS).not.toContain("tavily");
  });

  it("keeps optional MCP credential services aligned with preset optional flags", () => {
    const fromPresets = MCP_PROVIDER_PRESETS.flatMap((preset) =>
      preset.credentials
        .filter((credential) => credential.optional)
        .map((credential) => credential.service),
    ).sort();
    expect([...mcpOptionalCredentials].sort()).toEqual(fromPresets);
  });

  it("keeps search result limit manifest aligned with preset maxResultsArg", () => {
    const fromPresets = presetDeclaredResultLimitTargets().map((target) => ({
      presetId: target.presetId,
      maxResultsArg: target.maxResultsArg,
    }));
    const fromManifest = mcpSearchResultLimitManifest.map((target) => ({
      presetId: target.presetId,
      maxResultsArg: target.maxResultsArg,
    }));
    expect(fromManifest).toEqual(fromPresets);
  });

  it("uses one LLM builtin manifest for Rust and frontend fallback", () => {
    const ids = builtinLlmProviders.map((provider) => provider.id);
    expect(ids).toContain("deepseek");
    expect(ids).toContain("mimo");
    expect(ids).not.toContain("custom");

    const rustLoader = readFileSync("src-tauri/src/config_manifest.rs", "utf8");
    expect(rustLoader).toContain("llm-builtin-providers.json");
    expect(rustLoader).toContain("mcp-optional-credential-services.json");
    expect(rustLoader).toContain("mcp-search-result-limit-manifest.json");

    const frontend = readFileSync(
      "src/components/settings/LlmRoutingSection.tsx",
      "utf8",
    );
    expect(frontend).toContain("llm-builtin-providers.json");
    expect(frontend).not.toMatch(/id:\s*"deepseek"/);
  });
});
