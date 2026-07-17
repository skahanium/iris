import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("classified Agent Run storage contract", () => {
  it("keeps classified note commands outside SessionManager and delegates Markdown persistence", () => {
    const source = read("src-tauri/src/commands/classified.rs");

    expect(source).not.toContain("SessionManager");
    expect(source).toContain("classified_io");
    expect(source).toContain("decrypt_cef");
    expect(source).toContain("VaultKey");
    expect(source).toContain("require_unlocked");
    expect(source).toContain("NoteWriteService::create");
    expect(source).toContain("NoteWriteService::write");
    expect(source).not.toContain("classified_io::encrypt_cef");
  });

  it("stores normal conversations behind opaque scene-free session keys", () => {
    const source = read(
      "src-tauri/src/ai_runtime/normal_session_repository.rs",
    );

    expect(source).toContain("run_session:");
    expect(source).toContain(
      "INSERT INTO sessions (session_key, created_at, updated_at)",
    );
    expect(source).not.toContain("INSERT INTO sessions (session_key, scene");
    expect(source).not.toContain("note_path");
    expect(source).toContain("pub(crate) struct NormalSessionMessage");
    expect(source).toContain("content_parts: Option<String>");
  });

  it("retains the legacy CEF schema without using it for new classified runs", () => {
    const source = read("src-tauri/src/ai_runtime/classified_session.rs");
    const schema =
      source
        .split("pub struct ClassifiedAiThread")[1]
        ?.split("/// A single")[0] ?? "";

    for (const field of [
      "thread_id",
      "messages",
      "turns",
      "runs",
      "events",
      "evidence",
    ]) {
      expect(schema).toContain(field);
    }
    expect(schema).not.toContain("document_path");
    expect(source).toContain("CLASSIFIED_SESSION_SCHEMA_VERSION: u32 = 3");
    expect(source).toContain("classified_run_accept");
    expect(source).toContain("SecurityDomain::Classified");
  });

  it("uses UUID CEF filenames and verifies encrypted writes before replacement", () => {
    const source = read("src-tauri/src/ai_runtime/classified_session.rs");
    const pathFn =
      source.split("fn thread_file_path")[1]?.split("\n}")[0] ?? "";
    const atomicWrite = source.split("fn write_thread_atomically")[1] ?? "";

    expect(pathFn).toContain("{thread_id}.cef");
    expect(pathFn).not.toContain("title");
    expect(source).toContain("Uuid::parse_str");
    expect(atomicWrite).toContain("encrypt_cef");
    expect(atomicWrite).toContain("decrypt_cef");
    expect(atomicWrite).toContain("fs::rename");
  });

  it("routes new classified runs through volatile document-bound capabilities", () => {
    const commands = read("src-tauri/src/commands/assistant_commands.rs");
    const volatileStore = read(
      "src-tauri/src/ai_runtime/classified_ephemeral.rs",
    );

    expect(commands).toContain("SecurityDomain::Classified =>");
    expect(commands).toContain("assistant_classified_context_open");
    expect(commands).toContain("assistant_classified_run_take_result");
    expect(commands).not.toContain("RunIntake::start_classified");
    expect(commands).not.toContain("classified_run_get");
    expect(commands).not.toContain("classified_ai_thread_list");
    expect(volatileStore).toContain("Zeroizing<String>");
    expect(volatileStore).toContain("take_result");
    expect(volatileStore).not.toContain("write_thread_atomically");
    expect(commands).not.toContain("assistant_execute");
    expect(commands).not.toContain("harness_resume");
  });
});
