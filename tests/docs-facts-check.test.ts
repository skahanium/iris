import { spawnSync } from "node:child_process";
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
});
