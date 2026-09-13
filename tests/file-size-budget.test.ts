import { spawnSync } from "node:child_process";
import path from "node:path";
import { describe, expect, it } from "vitest";
import {
  DEFAULT_MAX_LINES,
  SPLIT_QUEUE,
  collectSourceFiles,
  countLines,
  evaluateBudgets,
} from "../scripts/file-size-budget.mjs";

const repoRoot = process.cwd();
const scriptPath = path.join(repoRoot, "scripts", "file-size-budget.mjs");

function runSizeCheck() {
  return spawnSync("node", [scriptPath], { cwd: repoRoot, encoding: "utf8" });
}

describe("size:check — first-party file budget", () => {
  it("exits 0 for the current repository", () => {
    const result = runSizeCheck();
    expect(result.status, `size:check failed:\n${result.stderr}`).toBe(0);
    expect(result.stdout).toContain("size:check PASSED");
  });

  it("counts lines without inventing one for the trailing newline", () => {
    expect(countLines("")).toBe(0);
    expect(countLines("a")).toBe(1);
    expect(countLines("a\n")).toBe(1);
    expect(countLines("a\nb\n")).toBe(2);
  });

  it("fails a file that grows past the default budget", () => {
    const { violations, stalePins } = evaluateBudgets(
      [{ path: "src/new-mega-file.ts", lines: DEFAULT_MAX_LINES + 1 }],
      { pinned: {} },
    );
    expect(stalePins).toEqual([]);
    expect(violations).toEqual([
      {
        path: "src/new-mega-file.ts",
        lines: DEFAULT_MAX_LINES + 1,
        max: DEFAULT_MAX_LINES,
        pinned: false,
      },
    ]);
  });

  it("holds a pinned file to its pin rather than the default", () => {
    const pinned = { "src-tauri/src/big.rs": 3000 };
    const atPin = evaluateBudgets(
      [{ path: "src-tauri/src/big.rs", lines: 3000 }],
      { pinned },
    );
    expect(atPin.violations).toEqual([]);
    expect(atPin.stalePins).toEqual([]);

    const overPin = evaluateBudgets(
      [{ path: "src-tauri/src/big.rs", lines: 3001 }],
      { pinned },
    );
    expect(overPin.violations).toHaveLength(1);
    expect(overPin.violations[0]).toMatchObject({ pinned: true, max: 3000 });
  });

  it("reports a split-queue entry that is no longer needed", () => {
    const pinned = { "src-tauri/src/split-done.rs": 4000 };
    const { violations, stalePins } = evaluateBudgets(
      [{ path: "src-tauri/src/split-done.rs", lines: 900 }],
      { pinned },
    );
    expect(violations).toEqual([]);
    expect(stalePins).toEqual([
      {
        path: "src-tauri/src/split-done.rs",
        reason: `now 900 lines, at or under the ${DEFAULT_MAX_LINES}-line default`,
      },
    ]);

    const missing = evaluateBudgets([], { pinned });
    expect(missing.stalePins).toEqual([
      { path: "src-tauri/src/split-done.rs", reason: "file no longer exists" },
    ]);
  });

  it("keeps every split-queue pin above the default and unique", () => {
    const paths = Object.keys(SPLIT_QUEUE);
    expect(new Set(paths).size).toBe(paths.length);
    for (const [pinnedPath, max] of Object.entries(SPLIT_QUEUE)) {
      expect(
        max,
        `${pinnedPath} must exceed the default budget`,
      ).toBeGreaterThan(DEFAULT_MAX_LINES);
    }
  });

  it("measures the files it claims to measure", () => {
    const entries = collectSourceFiles(repoRoot);
    const paths = entries.map((entry) => entry.path);
    expect(paths).toContain("src-tauri/src/ai_runtime/agent_capacity_eval.rs");
    expect(paths).toContain("src/lib/ipc.ts");
    expect(paths.every((entry) => !entry.includes("node_modules"))).toBe(true);
  });
});
