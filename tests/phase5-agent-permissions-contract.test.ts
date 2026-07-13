import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Run tool permission contract", () => {
  it("mirrors permission atoms, decisions, risk, and scope summaries across Rust and TypeScript", () => {
    const aiTypes = read("src/types/ai.ts");
    const ipcTypes = read("src/types/ipc.ts");
    const permissions = read("src-tauri/src/ai_runtime/agent_permissions.rs");
    const pipeline = read(
      "src-tauri/src/ai_runtime/tool_execution_pipeline.rs",
    );
    const loop = read("src-tauri/src/ai_runtime/run_tool_loop.rs");

    for (const token of [
      "AgentPermissionAtom",
      "PermissionRiskLevel",
      "PermissionScopeKind",
      "PermissionDecision",
      "PermissionEffectSummary",
      "vault.write.patch",
      "web.fetch",
      "secret.read_plaintext",
    ]) {
      expect(aiTypes).toContain(token);
      expect(permissions).toContain(token);
    }

    expect(aiTypes).toContain("scopeSummary");
    expect(aiTypes).toContain("reversibleBy");
    expect(aiTypes).toContain("blockedReason");
    expect(ipcTypes).toContain("permissionEffects");
    expect(ipcTypes).toContain("permissionDecision");
    expect(permissions).toContain("preflight_tool_permission");
    expect(pipeline).toContain("pub run_id: &'a str");
    expect(pipeline).toContain("record_permission_decision_audit");
    expect(loop).toContain("ToolExecutionGate");
    expect(loop).toContain("accepted.run_id");
    expect(loop).not.toContain("Harness");
  });

  it("migrates permission audit identity from request_id to run_id in the cutover", () => {
    const migrate = read("src-tauri/src/storage/migrate.rs");
    const up = read("src-tauri/migrations/051_agent_harness_cutover.sql");
    const down = read(
      "src-tauri/migrations/051_agent_harness_cutover.down.sql",
    );

    expect(migrate).toContain("051_agent_harness_cutover");
    expect(up).toContain("agent_permission_audit__cutover");
    expect(up).toContain(
      "run_id            TEXT NOT NULL REFERENCES agent_runs(run_id)",
    );
    expect(up).not.toContain("note_body");
    expect(up).not.toContain("clipboard_body");
    expect(up).not.toContain("api_key");
    expect(down).toContain("agent_permission_audit__legacy");
    expect(down).toContain("request_id");
  });
});
