import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("classified Agent Run storage contract", () => {
  it("keeps classified note commands outside the normal SessionManager", () => {
    const source = read("src-tauri/src/commands/classified.rs");

    expect(source).not.toContain("SessionManager");
    expect(source).toContain("classified_io");
    expect(source).toContain("encrypt_cef");
    expect(source).toContain("decrypt_cef");
    expect(source).toContain("VaultKey");
    expect(source).toContain("require_unlocked");
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

  it("persists classified conversation lifecycle only in the CEF thread schema", () => {
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

  it("routes classified run start, replay, and history through the unified domain boundary", () => {
    const commands = read("src-tauri/src/commands/assistant_commands.rs");

    expect(commands).toContain("SecurityDomain::Classified =>");
    expect(commands).toContain("RunIntake::start_classified");
    expect(commands).toContain("classified_run_get");
    expect(commands).toContain("classified_ai_thread_list");
    expect(commands).not.toContain("assistant_execute");
    expect(commands).not.toContain("harness_resume");
  });
});
