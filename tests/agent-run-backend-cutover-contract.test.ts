import { existsSync, readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Agent Run backend cutover contract", () => {
  it("registers only unified Run and opaque domain-routed session entry points", () => {
    const source = read("src-tauri/src/lib.rs");

    for (const command of [
      "commands::assistant_commands::assistant_run_start",
      "commands::assistant_commands::assistant_run_control",
      "commands::assistant_commands::assistant_run_get",
      "commands::assistant_commands::assistant_session_list",
      "commands::assistant_commands::assistant_session_load",
      "commands::assistant_commands::assistant_session_rename",
      "commands::assistant_commands::assistant_session_delete",
      "commands::assistant_commands::assistant_session_retract",
    ]) {
      expect(source).toContain(command);
    }

    for (const retired of [
      "commands::assistant_commands::assistant_execute",
      "commands::ai_commands::context_assemble",
      "commands::ai_commands::ai_send_message",
      "commands::ai_commands::tool_confirm",
      "commands::ai_commands::session_list",
      "commands::ai_commands::agent_task_resume",
      "commands::ai_commands::harness_resume",
      "commands::writing_commands::writing_execute",
      "commands::citation_commands::citation_check",
      "commands::organize_commands::organize_execute",
      "commands::document_commands::chapter_writing_execute",
      "commands::document_commands::document_check_execute",
    ]) {
      expect(source).not.toContain(retired);
    }
  });

  it("physically removes retired execution command modules and exports", () => {
    for (const path of [
      "src-tauri/src/commands/citation_commands.rs",
      "src-tauri/src/commands/organize_commands.rs",
      "src-tauri/src/commands/document_commands.rs",
      "src-tauri/src/commands/writing_commands.rs",
      "src-tauri/src/ai_runtime/session.rs",
      "src-tauri/src/ai_harness/harness/run.rs",
    ]) {
      expect(existsSync(path)).toBe(false);
    }

    for (const path of [
      "src-tauri/src/commands/assistant_commands.rs",
      "src-tauri/src/commands/ai_commands.rs",
    ]) {
      const source = read(path);
      for (const signature of [
        "assistant_execute",
        "context_assemble",
        "ai_send_message",
        "tool_confirm",
        "session_list",
        "agent_task_resume",
        "harness_resume",
      ]) {
        expect(source).not.toContain(`pub async fn ${signature}`);
      }
    }
  });

  it("keeps normal sessions and file rename logic free of scene and document bindings", () => {
    const file = read("src-tauri/src/commands/file.rs");
    const sessions = read(
      "src-tauri/src/ai_runtime/normal_session_repository.rs",
    );

    expect(file).not.toContain("UPDATE sessions SET note_path");
    expect(file).not.toContain("scene || ':' || ?1");
    expect(file).not.toContain("cascade_rename_sessions");
    expect(sessions).toContain("Scene-free normal-domain Session persistence");
    expect(sessions).not.toContain("note_path");
    expect(sessions).not.toContain("scene:");
  });
});
