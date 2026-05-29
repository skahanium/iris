import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("assembled context execution plan", () => {
  it("includes execution_plan on AssembledContext in TS and Rust", () => {
    expect(read("src/types/ai.ts")).toContain("execution_plan?: ExecutionPlan");
    expect(read("src-tauri/src/ai_runtime/mod.rs")).toContain("execution_plan");
    expect(read("src-tauri/src/ai_runtime/execution_plan.rs")).toContain(
      "execution_plan_from_context_plan",
    );
  });

  it("context_assemble attaches planner output to the response", () => {
    const source = read("src-tauri/src/commands/ai_commands.rs");
    expect(source).toContain("execution_plan_from_context_plan");
    expect(source).toContain("execution_plan");
    expect(source).toContain("sub_queries.len() > 1");
  });
});
