import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const workspaceRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const mode = process.argv[2];

if (mode === "live") {
  console.error(
    "agent_eval_live_requires_an_explicit_approved_profile; run the deterministic suites until live preflight is configured.",
  );
  process.exit(2);
}

if (mode !== "smoke" && mode !== "full") {
  console.error("usage: node scripts/agent-eval.mjs <smoke|full|live>");
  process.exit(2);
}

const testName =
  "ai_runtime::agent_capacity_eval_tests::deterministic_command_entrypoint_writes_only_the_strict_summary_when_requested";
const result = spawnSync(
  "cargo",
  [
    "test",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "--lib",
    testName,
    "--",
    "--exact",
    "--nocapture",
  ],
  {
    cwd: workspaceRoot,
    env: {
      ...process.env,
      IRIS_AGENT_EVAL_MODE: mode,
    },
    stdio: "inherit",
  },
);

if (result.error) {
  console.error("agent_eval_runner_failed");
  process.exit(1);
}
if (result.status !== 0) {
  process.exit(result.status ?? 1);
}

const output = path.join(
  workspaceRoot,
  "target",
  "agent-eval",
  mode === "smoke" ? "core-smoke.json" : "core-full.json",
);
if (!existsSync(output)) {
  console.error("agent_eval_summary_missing");
  process.exit(1);
}

console.log(`agent_eval_summary=${path.relative(workspaceRoot, output)}`);
