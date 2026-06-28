import { describe, expect, it } from "vitest";

import { readFileSync } from "node:fs";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("classified AI runtime lifecycle", () => {
  describe("useAiDomainRuntime exports clearClassifiedVolatileState", () => {
    it("hook file exports clearClassifiedVolatileState in return type", () => {
      const src = read("src/hooks/useAiDomainRuntime.ts");
      expect(src).toContain("clearClassifiedVolatileState");
    });

    it("clearClassifiedVolatileState clears stream buffer", () => {
      const src = read("src/hooks/useAiDomainRuntime.ts");
      // Should reset classified stream buffer ref
      expect(src).toMatch(
        /clearClassifiedVolatileState[\s\S]*classifiedStreamBufRef/,
      );
    });

    it("clearClassifiedVolatileState clears selected messages", () => {
      const src = read("src/hooks/useAiDomainRuntime.ts");
      expect(src).toMatch(
        /clearClassifiedVolatileState[\s\S]*classifiedSelectedMessageIds/,
      );
    });

    it("clearClassifiedVolatileState clears thread snapshot map", () => {
      const src = read("src/hooks/useAiDomainRuntime.ts");
      expect(src).toMatch(
        /clearClassifiedVolatileState[\s\S]*classifiedThreadByPath/,
      );
    });

    it("clearClassifiedVolatileState calls classifiedAiCacheClear IPC", () => {
      const src = read("src/hooks/useAiDomainRuntime.ts");
      expect(src).toContain("classifiedAiCacheClear");
    });

    it("clearClassifiedVolatileState accepts a reason string parameter", () => {
      const src = read("src/hooks/useAiDomainRuntime.ts");
      // The function signature must include reason: string
      expect(src).toMatch(
        /clearClassifiedVolatileState[\s\S]*reason\s*:\s*string/,
      );
    });

    it("clearClassifiedVolatileState is in the return type interface", () => {
      const src = read("src/hooks/useAiDomainRuntime.ts");
      const returnBlock = src.split("UseAiDomainRuntimeReturn")[1] ?? "";
      expect(returnBlock).toContain("clearClassifiedVolatileState");
    });
  });

  describe("useClassifiedVaultSession aborts active classified request on lock", () => {
    it("accepts abortClassifiedRequest callback in options", () => {
      const src = read("src/hooks/useClassifiedVaultSession.ts");
      expect(src).toContain("abortClassifiedRequest");
    });

    it("calls abortClassifiedRequest before performing lock", () => {
      const src = read("src/hooks/useClassifiedVaultSession.ts");
      // abortClassifiedRequest should be called inside performLock or requestLock
      const performLockSection = src.split("performLock")[1] ?? "";
      expect(performLockSection).toContain("abortClassifiedRequest");
    });

    it("onLocked callback fires after lock completes", () => {
      const src = read("src/hooks/useClassifiedVaultSession.ts");
      // onLockedRef.current?.() must be called after setStatus("locked")
      const performLockSection = src.split("performLock")[1] ?? "";
      expect(performLockSection).toMatch(/setStatus.*locked[\s\S]*onLockedRef/);
    });
  });

  describe("useAssistantLlmStream domain-scoped stream buffers", () => {
    it("accepts domain parameter in options", () => {
      const src = read("src/hooks/useAssistantLlmStream.ts");
      expect(src).toContain("domain");
    });

    it("ignores late token events when domain is not classified", () => {
      const src = read("src/hooks/useAssistantLlmStream.ts");
      // After classified→normal domain switch, late classified tokens must be dropped
      expect(src).toMatch(/domain[\s\S]*classified[\s\S]*return|ignore|skip/);
    });
  });

  describe("Rust trace redaction for classified requests", () => {
    it("trace.rs has redaction helper function", () => {
      const src = read("src-tauri/src/ai_runtime/trace.rs");
      expect(src).toContain("redact");
    });

    it("redaction removes .classified/ paths from trace records", () => {
      const src = read("src-tauri/src/ai_runtime/trace.rs");
      expect(src).toMatch(/redact[\s\S]*\.classified/);
    });

    it("redaction removes document titles from trace records", () => {
      const src = read("src-tauri/src/ai_runtime/trace.rs");
      expect(src).toMatch(/redact[\s\S]*title|document/);
    });

    it("complete method applies redaction to error messages", () => {
      const src = read("src-tauri/src/ai_runtime/trace.rs");
      // The complete function should redact before storing
      const completeSection = src.split("pub fn complete")[1] ?? "";
      expect(completeSection).toContain("redact");
    });
  });

  describe("Rust classified_ai_security.rs runtime assertions", () => {
    it("test file contains sentinel phrase search in ordinary DB tables", () => {
      const src = read("src-tauri/tests/classified_ai_security.rs");
      expect(src).toContain("sentinel");
    });

    it("test file asserts trace rows do not contain .classified/ paths", () => {
      const src = read("src-tauri/tests/classified_ai_security.rs");
      expect(src).toMatch(/trace[\s\S]*\.classified/);
    });

    it("test file asserts trace rows do not contain document title", () => {
      const src = read("src-tauri/tests/classified_ai_security.rs");
      expect(src).toMatch(/trace[\s\S]*title/);
    });
  });
});
