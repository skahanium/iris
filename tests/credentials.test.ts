import { describe, expect, it } from "vitest";

import {
  MINIMAX_CREDENTIAL_SERVICE,
  llmCredentialService,
} from "@/lib/credentials";

describe("credential service names", () => {
  it("uses iris.minimax for MiniMax web search", () => {
    expect(MINIMAX_CREDENTIAL_SERVICE).toBe("iris.minimax");
  });

  it("scopes LLM keys per provider", () => {
    expect(llmCredentialService("openai")).toBe("iris.llm.openai");
    expect(llmCredentialService("deepseek")).toBe("iris.llm.deepseek");
  });
});
