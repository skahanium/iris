import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return "";
  }
}

describe("fetch_web_page integration surface", () => {
  it("registers fetch_web_page tool and UI labels", () => {
    const executor = read("src-tauri/src/ai_runtime/tool_executor.rs");
    expect(executor).toContain("fetch_web_page");

    const catalog = read("src-tauri/src/ai_runtime/tool_catalog.rs");
    expect(catalog).toContain('name: "fetch_web_page"');
    expect(catalog).toContain("requires_confirmation: true");

    const dispatch = read("src-tauri/src/ai_runtime/tool_dispatch.rs");
    expect(dispatch).toContain("fetch_web_page_tool");

    const names = read("src/lib/tool-display-names.ts");
    expect(names).toContain("fetch_web_page");

    const dialog = read("src/components/ai/ToolConfirmDialog.tsx");
    expect(dialog).toContain("fetch_web_page");
  });

  it("registers skills management tools in catalog and dispatch", () => {
    const catalog = read("src-tauri/src/ai_runtime/tool_catalog.rs");
    expect(catalog).toContain('name: "skills_list"');
    expect(catalog).toContain('name: "skills_install"');
    expect(catalog).toContain('name: "skills_uninstall"');
    expect(catalog).toContain('name: "skills_toggle"');

    const dispatch = read("src-tauri/src/ai_runtime/tool_dispatch.rs");
    expect(dispatch).toContain("skills_install_tool");
    expect(dispatch).toContain("skills_list_tool");

    const dialog = read("src/components/ai/ToolConfirmDialog.tsx");
    expect(dialog).toContain("skills_install");
  });

  it("ipc skillsInstall accepts registry source field", () => {
    const ipc = read("src/lib/ipc.ts");
    expect(ipc).toContain("registry?: string");
  });

  it("assistant panel shows skill install success notice helper", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    expect(panel).toContain("skillInstallSuccessNotice");
    expect(panel).toContain('pendingConfirm?.tool_name === "skills_install"');
  });
});
