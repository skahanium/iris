import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const repoRoot = process.cwd();
const scriptPath = path.join(repoRoot, "scripts", "docs-facts-check.mjs");

function runDocsCheck(args: string[] = []) {
  const result = spawnSync("node", [scriptPath, ...args], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  return {
    exitCode: result.status ?? 1,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
  };
}

describe("docs:check — document facts verification", () => {
  it("exits 0 when all document facts are consistent", () => {
    const result = runDocsCheck();
    expect(result.exitCode, `docs:check failed:\n${result.stderr}`).toBe(0);
  });

  it("exits non-zero when ARCHITECTURE.md migration count differs from actual migrations", () => {
    // Simulate stale migration count by checking with a wrong expected count.
    const result = runDocsCheck(["--expected-migration-group", "999"]);
    expect(result.exitCode).not.toBe(0);
  });

  it("detects stale 'OS 凭据管理器' references in docs/", () => {
    // Simulate by passing a flag that forces scanning for the phrase.
    const result = runDocsCheck(["--forbidden-phrase", "OS 凭据管理器"]);
    // The codebase should be clean now — verify script reports no such phrase.
    expect(result.exitCode).toBe(0);
  });

  it("verifies the scheduler IPC contract rather than retired reindex commands", () => {
    const source = readFileSync(scriptPath, "utf8");

    expect(source).toContain("embedding_scheduler_status");
    expect(source).not.toContain("missing search_embedding_status entry");
  });

  it("guards the docs index and IPC contract against retired architecture", () => {
    const source = readFileSync(scriptPath, "utf8");

    expect(source).toContain("checkRetiredArchitectureReferences");
    expect(source).toContain("checkAgentHarnessDocumentation");
    expect(source).toContain("2026-08-pre-unification");
    expect(source).toContain("2026-09-15-pre-reform");
    expect(source).toContain("version_cleanup_cmd");
  });

  it("keeps the retired Harness construction set archived, not active", () => {
    // `agent-harness/` 被**重建**为现行体系后，同名入口 `README.md` 重新存在，它是
    // 现行入口（`P01`）而不是被归档的那一套。旧施工集的判据因此不是「入口不存在」，
    // 而是「旧施工集只在 archive 里、根目录不残留被取代的路径」。这条断言原先写的是
    // 重建前的形状，在 787ff28a 把新入口加回来之后就一直与 docs:check 相反。
    expect(existsSync(path.join(repoRoot, "agent-harness", "README.md"))).toBe(
      true,
    );
    expect(
      existsSync(
        path.join(
          repoRoot,
          "agent-harness",
          "archive",
          "2026-09-15-pre-reform",
          "MANIFEST.md",
        ),
      ),
    ).toBe(true);
    expect(
      existsSync(
        path.join(
          repoRoot,
          "agent-harness",
          "archive",
          "2026-08-pre-unification",
          "MANIFEST.md",
        ),
      ),
    ).toBe(true);
    expect(existsSync(path.join(repoRoot, "refactor"))).toBe(false);
    expect(existsSync(path.join(repoRoot, "structured-tools"))).toBe(false);
    expect(existsSync(path.join(repoRoot, "REFACTOR.md"))).toBe(false);
  });

  it("guards current RAG, release-platform, and security claims against factual drift", () => {
    const source = readFileSync(scriptPath, "utf8");

    expect(source).toContain("checkReleaseDocumentationFacts");
    expect(source).toContain("semantic-search.md");
    expect(source).toContain("rag-v2-broker-evaluation.md");
    expect(source).toContain("SECURITY.md");
    expect(source).toContain("sqlite-vec");
    expect(source).toContain("macOS + Windows");
  });

  it("checks the frozen RAG fixture contract rather than only its document heading", () => {
    const source = readFileSync(scriptPath, "utf8");

    expect(source).toContain("checkRagFixtureContract");
    expect(source).toContain("fixture-metadata.json");
    expect(source).toContain("historical_frozen");
    expect(source).toContain("fixtureVersion");
    expect(source).toContain("currentEvaluationVersion");
    expect(source).toContain("fixtureDocumentFacts");
  });
});
