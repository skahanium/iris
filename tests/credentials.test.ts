import { describe, expect, it } from "vitest";

import {
  BING_SEARCH_CREDENTIAL_SERVICE,
  llmCredentialService,
} from "@/lib/credentials";

describe("credential service names", () => {
  it("uses iris.bing.search for Bing web search", () => {
    expect(BING_SEARCH_CREDENTIAL_SERVICE).toBe("iris.bing.search");
  });

  it("scopes LLM keys per provider", () => {
    expect(llmCredentialService("openai")).toBe("iris.llm.openai");
    expect(llmCredentialService("deepseek")).toBe("iris.llm.deepseek");
  });
});
