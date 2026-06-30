import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return "";
  }
}

describe("AI reign-in target surface", () => {
  it("does not expose legacy fetch, skill-install, or MCP management tools", () => {
    const catalogWeb = read("src-tauri/src/ai_runtime/tool_catalog/web.rs");
    const catalogSkills = read(
      "src-tauri/src/ai_runtime/tool_catalog/skills.rs",
    );
    const dispatch = read("src-tauri/src/ai_runtime/tool_dispatch_impl.rs");
    const names = read("src/lib/tool-display-names.ts");
    const dialog = read("src/components/ai/ToolConfirmDialog.tsx");

    for (const legacy of [
      "fetch_web_page",
      "web_fetch_batch",
      "readability_fetch",
      "rendered_fetch",
      "skills_install",
      "skills_prepare_workspace",
      "mcp_runtime_capability_call",
      "mcp_server_catalog_upsert",
      "mcp_runtime_profile_upsert",
      "mcp_runtime_tools_list",
      "mcp_runtime_health_check",
    ]) {
      expect(catalogWeb + catalogSkills).not.toContain(`name: "${legacy}"`);
      expect(dispatch).not.toContain(`"${legacy}" =>`);
      expect(names).not.toContain(legacy);
      expect(dialog).not.toContain(`case "${legacy}"`);
    }
  });

  it("keeps web_search as the single model-visible network tool", () => {
    const catalogWeb = read("src-tauri/src/ai_runtime/tool_catalog/web.rs");

    expect(catalogWeb).toContain('name: "web_search"');
    expect(catalogWeb).toContain('"urls"');
    expect(catalogWeb).toContain("WebEvidenceBroker");
  });

  it("does not expose SkillHub direct install routing", () => {
    const routing = read("src/lib/assistant-routing.ts");
    const taskplan = read("src/lib/assistant-taskplan.ts");
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    const notice = read("src/lib/skill-install-notice.ts");

    expect(routing + taskplan + panel + notice).not.toContain("SkillHub");
    expect(routing + taskplan + panel + notice).not.toContain("skills_install");
    expect(panel).not.toContain("skillInstallSuccessNotice");
  });
});
