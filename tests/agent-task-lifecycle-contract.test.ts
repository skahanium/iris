import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

function functionSlice(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("Agent Task Runtime Phase D lifecycle contract", () => {
  it("cache clear and vault switch abort recoverable task state without deleting notes", () => {
    const runtime = read("src-tauri/src/ai_runtime/agent_task.rs");
    const aiCommands = read("src-tauri/src/commands/ai_commands.rs");
    const fileCommands = read("src-tauri/src/commands/file.rs");
    const appState = read("src-tauri/src/app.rs");

    expect(runtime).toContain("pub fn abort_recoverable_tasks");
    expect(aiCommands).toContain("abort_recoverable_tasks(");
    expect(fileCommands).toContain("abort_recoverable_tasks(");
    expect(appState).toContain("VAULT_RESET");

    const cacheClear = functionSlice(
      aiCommands,
      "pub async fn ai_cache_clear",
      "/// Return a durable Agent Task by id.",
    );
    expect(cacheClear).toContain("abort_recoverable_tasks(");
    expect(cacheClear).not.toContain("remove_file");
    expect(cacheClear).not.toContain("trash_document");
    expect(cacheClear).not.toContain("discard_document");

    const vaultSet = functionSlice(
      fileCommands,
      "pub fn vault_set",
      "#[tauri::command]\npub fn vault_get",
    );
    expect(vaultSet).toContain("abort_recoverable_tasks(");
    expect(vaultSet).not.toContain("remove_file");
    expect(vaultSet).not.toContain("trash_document");
    expect(vaultSet).not.toContain("discard_document");

    const vaultReset = functionSlice(
      appState,
      "fn clear_vault_setting",
      "fn load_vault_setting",
    );
    expect(vaultReset).toContain("abort_recoverable_tasks(");
    expect(vaultReset).not.toContain("remove_file");
    expect(vaultReset).not.toContain("trash_document");
    expect(vaultReset).not.toContain("discard_document");
  });

  it("frontend treats vault mismatch resume failures as non-recoverable", () => {
    const recovery = read("src/components/ai/AssistantErrorRecovery.tsx");
    const recoveryLib = read("src/lib/ai/resume-recovery.ts");
    const resumeHook = read(
      "src/components/ai/hooks/useAssistantHarnessResume.ts",
    );
    const header = read("src/components/ai/AssistantPanelHeader.tsx");
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");
    const history = read("src/components/ai/SessionHistoryDropdown.tsx");

    expect(recovery).toContain("isUnrecoverableResumeError");
    expect(recoveryLib).toContain("当前库已变更");
    expect(recoveryLib).toContain("vault scope changed");
    expect(recovery).toContain("!unrecoverable");
    expect(resumeHook).toContain("RESUME_PREFLIGHT_FAILED");
    expect(resumeHook).toContain("vault scope changed");
    expect(resumeHook).toContain("setPausedTaskId(null)");
    expect(header).not.toContain("onClearedAllSessions");
    expect(header).not.toContain("onClearedAll=");
    expect(history).not.toContain("sessionClearAll");
    expect(history).not.toContain("当前上下文");
    expect(panel).toContain("resetAssistantSessionState");
    expect(panel).toContain("setPausedTaskId(null)");
  });
});
