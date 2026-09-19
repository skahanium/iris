#!/usr/bin/env node
// File-size budget for first-party source.
//
// Why this exists: `agent_capacity_eval.rs` grew past 13k lines while nothing in
// CI could notice, and the same pattern had already produced a dozen 3k-line
// modules. A budget is a ratchet, not a style preference:
//
//   * every first-party source file must stay at or under `DEFAULT_MAX_LINES`;
//   * files that already exceed it are pinned in `SPLIT_QUEUE` at their current
//     size (rounded up to the next 100 lines) and may shrink but never grow;
//   * a pin must be deleted once its file is back under the default, so the
//     queue cannot rot into a permanent exemption list.
//
// Run: `npm run size:check`.

import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const defaultRoot = path.resolve(scriptDir, "..");

/** Any first-party source file above this must be split or pinned. */
export const DEFAULT_MAX_LINES = 2000;

/** Roots scanned by the gate, relative to the repository root. */
export const SCANNED_ROOTS = ["src", "src-tauri/src", "tests", "scripts"];

const SCANNED_EXTENSIONS = [".rs", ".ts", ".tsx", ".mjs"];

/**
 * Files above the default today, pinned at their current size. This is a
 * split queue, not an allow-list: each entry is expected to disappear.
 */
export const SPLIT_QUEUE = {
  "src-tauri/src/ai_runtime/agent_capacity_eval/test_support.rs": 9600,
  "src-tauri/src/ai_runtime/run_tool_loop.rs": 6800,
  "src-tauri/src/ai_runtime/agent_capacity_eval_tests.rs": 5900,
  "src-tauri/src/ai_runtime/run_engine_tests.rs": 4700,
  "src-tauri/src/ai_runtime/agent_tool_loop_tests.rs": 3900,
  "src-tauri/src/storage/migrate.rs": 3600,
  "src-tauri/src/ai_runtime/mcp_host_runtime.rs": 3600,
  "src-tauri/src/ai_runtime/run_intake_tests.rs": 3400,
  "src-tauri/src/ai_runtime/agent_run_repository.rs": 3200,
  "src-tauri/src/ai_runtime/normal_run_service_tests.rs": 2700,
  "src-tauri/src/ai_runtime/web_evidence_broker.rs": 2600,
  "src-tauri/src/commands/assistant_commands.rs": 2600,
  "src-tauri/src/feed/repository_tests.rs": 2400,
  "src-tauri/src/embedding/scheduler.rs": 2400,
  "src-tauri/src/ai_runtime/agent_run_repository_tests.rs": 2200,
  "src-tauri/src/commands/file.rs": 2200,
  "src-tauri/src/ai_runtime/run_context_tests.rs": 2200,
  "src-tauri/src/recycle/mod.rs": 2200,
};

/** Line count of a file, where a trailing newline does not start a new line. */
export function countLines(content) {
  if (content === "") return 0;
  const lines = content.split("\n");
  return content.endsWith("\n") ? lines.length - 1 : lines.length;
}

/**
 * Compare measured files against the budget.
 *
 * @param {{path: string, lines: number}[]} entries repository-relative files
 * @returns {{violations: object[], stalePins: object[]}}
 */
export function evaluateBudgets(
  entries,
  { defaultMax = DEFAULT_MAX_LINES, pinned = SPLIT_QUEUE } = {},
) {
  const violations = [];
  const stalePins = [];
  const byPath = new Map(entries.map((entry) => [entry.path, entry]));

  for (const entry of entries) {
    const max = Object.hasOwn(pinned, entry.path)
      ? pinned[entry.path]
      : defaultMax;
    if (entry.lines > max) {
      violations.push({
        path: entry.path,
        lines: entry.lines,
        max,
        pinned: Object.hasOwn(pinned, entry.path),
      });
    }
  }

  for (const pinnedPath of Object.keys(pinned)) {
    const entry = byPath.get(pinnedPath);
    if (!entry) {
      stalePins.push({ path: pinnedPath, reason: "file no longer exists" });
    } else if (entry.lines <= defaultMax) {
      stalePins.push({
        path: pinnedPath,
        reason: `now ${entry.lines} lines, at or under the ${defaultMax}-line default`,
      });
    }
  }

  return { violations, stalePins };
}

function walk(directory, collected) {
  for (const entry of readdirSync(directory)) {
    const full = path.join(directory, entry);
    const stats = statSync(full);
    if (stats.isDirectory()) {
      walk(full, collected);
    } else if (SCANNED_EXTENSIONS.includes(path.extname(entry))) {
      collected.push(full);
    }
  }
  return collected;
}

/** Measure every scanned first-party file. */
export function collectSourceFiles(root = defaultRoot) {
  const files = [];
  for (const relativeRoot of SCANNED_ROOTS) {
    const absoluteRoot = path.join(root, relativeRoot);
    try {
      walk(absoluteRoot, files);
    } catch {
      // A repository without one of the roots is still valid input.
    }
  }
  return files.map((filePath) => ({
    path: path.relative(root, filePath).split(path.sep).join("/"),
    lines: countLines(readFileSync(filePath, "utf8")),
  }));
}

function main() {
  const entries = collectSourceFiles();
  const { violations, stalePins } = evaluateBudgets(entries);

  if (violations.length === 0 && stalePins.length === 0) {
    process.stdout.write(
      `size:check PASSED (${entries.length} files; default budget ${DEFAULT_MAX_LINES} lines; ${Object.keys(SPLIT_QUEUE).length} pinned for split)\n`,
    );
    process.exit(0);
  }

  const problems = [];
  for (const violation of violations) {
    problems.push(
      violation.pinned
        ? `${violation.path}: ${violation.lines} lines exceeds its pinned budget of ${violation.max}; split the file instead of raising the pin`
        : `${violation.path}: ${violation.lines} lines exceeds the ${violation.max}-line budget; split the file or add a pinned budget with a split plan`,
    );
  }
  for (const stale of stalePins) {
    problems.push(
      `${stale.path}: stale split-queue entry (${stale.reason}); remove it from SPLIT_QUEUE`,
    );
  }

  process.stderr.write(`size:check FAILED (${problems.length} issue(s)):\n`);
  for (const problem of problems) {
    process.stderr.write(`  ✗ ${problem}\n`);
  }
  process.exit(1);
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  main();
}
