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
    expect(executor).toContain("requires_confirmation: true");

    const dispatch = read("src-tauri/src/ai_runtime/tool_dispatch.rs");
    expect(dispatch).toContain("fetch_web_page_tool");

    const names = read("src/lib/tool-display-names.ts");
    expect(names).toContain("fetch_web_page");

    const dialog = read("src/components/ai/ToolConfirmDialog.tsx");
    expect(dialog).toContain("fetch_web_page");
  });

  it("harness merges fetch_web_page packets", () => {
    const harnessRun = read("src-tauri/src/ai_harness/harness/run.rs");
    const harnessTools = read("src-tauri/src/ai_harness/harness/tools.rs");
    expect(harnessRun).toContain("fetch_web_page");
    expect(harnessRun).toContain("parse_tool_calls_from_content");
    expect(harnessRun).toContain("strip_tool_markup_from_visible");
    expect(harnessTools).toContain("fetch_web_page");
  });
});
