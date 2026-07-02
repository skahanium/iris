import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

import { invokeErrorMessage, llmCredentialService } from "@/lib/credentials";

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

  it("explains keyring failures as credential access problems", () => {
    expect(invokeErrorMessage("Keyring error")).toContain("系统凭据");
  });

  it("explains structured credential errors even when only code is present", () => {
    expect(invokeErrorMessage({ code: "credential" })).toContain("系统凭据");
  });
});
