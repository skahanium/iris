import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Phase5 Markdown Agent permission contract", () => {
  it("mirrors permission atoms, decisions, risk, and scope summaries across Rust and TS", () => {
    const aiTypes = read("src/types/ai.ts");
    const ipcTypes = read("src/types/ipc.ts");
    const rustPermissions = read(
      "src-tauri/src/ai_runtime/agent_permissions.rs",
    );
    const harnessRun = read("src-tauri/src/ai_harness/harness/run.rs");

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
      expect(rustPermissions).toContain(token);
    }

    expect(aiTypes).toContain("scopeSummary");
    expect(aiTypes).toContain("reversibleBy");
    expect(aiTypes).toContain("blockedReason");
    expect(rustPermissions).toContain("scope_summary");
    expect(rustPermissions).toContain("reversible_by");
    expect(rustPermissions).toContain("blocked_reason");
    expect(ipcTypes).toContain("permissionEffects");
    expect(harnessRun).toContain("preflight_tool_permission");
    expect(harnessRun).toContain("permission_effects");
  });

  it("registers Phase5 permission storage with reversible migration scripts", () => {
    const migrate = read("src-tauri/src/storage/migrate.rs");
    const up = read("src-tauri/migrations/027_agent_permissions.sql");
    const down = read("src-tauri/migrations/027_agent_permissions.down.sql");

    expect(migrate).toContain("027_agent_permissions");
    expect(up).toContain("agent_permission_grants");
    expect(up).toContain("agent_permission_audit");
    expect(up).not.toContain("note_body");
    expect(up).not.toContain("clipboard_body");
    expect(up).not.toContain("api_key");
    expect(down).toContain("DROP TABLE IF EXISTS agent_permission_audit");
    expect(down).toContain("DROP TABLE IF EXISTS agent_permission_grants");
  });
});
