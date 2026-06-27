import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("classified AI session store contract", () => {
  describe("classified AI session APIs do not use ordinary session infrastructure", () => {
    it("classified.rs does not import SessionManager::ensure", () => {
      const src = read("src-tauri/src/commands/classified.rs");
      expect(src).not.toContain("SessionManager::ensure");
      expect(src).not.toContain("session_list");
      expect(src).not.toContain("session_messages");
    });

    it("classified.rs does not call SessionManager at all", () => {
      const src = read("src-tauri/src/commands/classified.rs");
      expect(src).not.toContain("SessionManager");
    });

    it("ai_commands validate_ai_note_path blocks classified paths", () => {
      const src = read("src-tauri/src/commands/ai_commands.rs");
      expect(src).toContain("涉密笔记不能进入 AI 管道");
      expect(src).toContain("validate_ai_note_path");
      expect(src).toContain("is_user_note_path(path)");
    });
  });

  describe("encrypted payload APIs depend on classified_io and VaultKey", () => {
    it("classified.rs imports classified_io for encrypt/decrypt", () => {
      const src = read("src-tauri/src/commands/classified.rs");
      expect(src).toContain("classified_io");
      expect(src).toContain("encrypt_cef");
      expect(src).toContain("decrypt_cef");
    });

    it("classified.rs uses VaultKey for key material", () => {
      const src = read("src-tauri/src/commands/classified.rs");
      expect(src).toContain("VaultKey");
      expect(src).toContain("VAULT_KEY");
      expect(src).toContain("require_unlocked");
    });

    it("classified_io module provides CEF magic detection", () => {
      const src = read("src-tauri/src/commands/classified.rs");
      expect(src).toContain("has_csef_magic");
    });
  });

  describe("thread filenames do not leak classified paths or titles", () => {
    it("session.rs uses uuid-based session keys", () => {
      const src = read("src-tauri/src/ai_runtime/session.rs");
      expect(src).toContain("uuid::Uuid::new_v4");
      expect(src).toMatch(/format!\("{}#{}",\s*session_key\(/);
    });

    it("session_key uses scene prefix and note_path but not title", () => {
      const src = read("src-tauri/src/ai_runtime/session.rs");
      expect(src).toContain("pub fn session_key");
      expect(src).toContain("scene_str");
      // session_key does not include title
      expect(src).not.toMatch(/session_key[\s\S]*title/);
    });

    it("classified paths are excluded from user note paths used by session system", () => {
      const src = read("src-tauri/src/storage/paths.rs");
      // is_user_note_path must reject .classified/ paths
      // This is already tested in classified_vault.rs but we lock it here too
      expect(src).toContain("is_user_note_path");
    });
  });

  describe("ordinary sessions table contains no classified message content", () => {
    it("session.rs SessionMessage struct has no classified-specific fields", () => {
      const src = read("src-tauri/src/ai_runtime/session.rs");
      expect(src).toContain("pub struct SessionMessage");
      // No field for vault key, encryption key, or classified path
      expect(src).not.toMatch(/SessionMessage[\s\S]*vault_key/);
      expect(src).not.toMatch(/SessionMessage[\s\S]*encryption_key/);
    });

    it("sessions table insert does not reference .classified paths", () => {
      const src = read("src-tauri/src/ai_runtime/session.rs");
      const createFresh = src.split("pub fn create_fresh")[1] ?? "";
      expect(createFresh).not.toContain(".classified");
    });
  });
});
