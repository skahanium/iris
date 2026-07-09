import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

import {
  invokeErrorMessage,
  llmCredentialService,
  mcpCredentialService,
} from "@/lib/credentials";

const removedVendor = ["mini", "max"].join("");
const removedCredential = ["MINI", "MAX", "_CREDENTIAL_SERVICE"].join("");

describe("credential service names", () => {
  it("does not expose legacy vendor web-search credential service", () => {
    const source = readFileSync("src/lib/credentials.ts", "utf8");

    expect(source).not.toContain(removedCredential);
    expect(source).not.toContain(`iris.${removedVendor}`);
  });

  it("scopes LLM keys per provider", () => {
    expect(llmCredentialService("openai")).toBe("iris.llm.openai");
    expect(llmCredentialService("deepseek")).toBe("iris.llm.deepseek");
  });

  it("scopes MCP keys separately from LLM providers", () => {
    expect(mcpCredentialService("anysearch")).toBe("iris.mcp.anysearch");
    expect(mcpCredentialService("jina")).toBe("iris.mcp.jina");
  });

  it("keeps credentials off the OS keychain backend to avoid system password prompts", () => {
    const cargo = readFileSync("src-tauri/Cargo.toml", "utf8");
    const credentials = readFileSync("src-tauri/src/credentials.rs", "utf8");

    expect(cargo).not.toContain("keyring");
    expect(credentials).not.toContain("keyring::");
    expect(credentials).not.toContain("KeyringCredentialBackend");
  });

  it("explains credential backend failures as re-save problems", () => {
    expect(invokeErrorMessage({ code: "credential" })).toContain(
      "重新输入并保存",
    );
  });

  it("explains structured credential errors even when only code is present", () => {
    expect(invokeErrorMessage({ code: "credential" })).toContain(
      "重新输入并保存",
    );
  });
});
