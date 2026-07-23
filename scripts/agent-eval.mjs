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
  const action = process.argv[3];
  if (action === "pilot") {
    const approveIndex = process.argv.indexOf("--approve");
    const approvedProfile =
      approveIndex >= 0 ? process.argv[approveIndex + 1] : undefined;
    if (!approvedProfile || !/^profile-[0-9a-f]{12}$/.test(approvedProfile)) {
      console.error("agent_eval_live_requires_an_explicit_approved_profile");
      process.exit(2);
    }
    console.error("agent_eval_live_pilot_requires_user_cost_checkpoint");
    process.exit(2);
  }
  if (action !== "preflight" || process.argv.length !== 4) {
    console.error(
      "agent_eval_live_requires_preflight_or_an_explicit_approved_profile",
    );
    process.exit(2);
  }

  const sourceDatabase =
    process.env.IRIS_AGENT_EVAL_SOURCE_DB ||
    path.join(
      process.env.IRIS_DATA_DIR ||
        path.join(workspaceRoot, ".iris-dev", "app-data"),
      "iris.db",
    );
  if (!existsSync(sourceDatabase)) {
    console.error("agent_eval_live_preflight_source_missing");
    process.exit(2);
  }
  const safeEnvironment = Object.fromEntries(
    Object.entries(process.env).filter(
      ([key]) =>
        !/(?:API[_-]?KEY|TOKEN|SECRET|PASSWORD|AUTHORIZATION|CREDENTIAL)/i.test(
          key,
        ),
    ),
  );
  const result = spawnSync(
    "cargo",
    [
      "test",
      "--manifest-path",
      "src-tauri/Cargo.toml",
      "--lib",
      "ai_runtime::agent_capacity_eval_tests::live_preflight_command_entrypoint_writes_only_the_anonymous_report_when_requested",
      "--",
      "--exact",
      "--nocapture",
    ],
    {
      cwd: workspaceRoot,
      env: {
        ...safeEnvironment,
        IRIS_AGENT_EVAL_LIVE_ACTION: "preflight",
        IRIS_AGENT_EVAL_SOURCE_DB: sourceDatabase,
      },
      stdio: "inherit",
    },
  );
  if (result.error) {
    console.error("agent_eval_live_preflight_runner_failed");
    process.exit(1);
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
  const output = path.join(
    workspaceRoot,
    "target",
    "agent-eval",
    "live-preflight.json",
  );
  if (!existsSync(output)) {
    console.error("agent_eval_live_preflight_summary_missing");
    process.exit(1);
  }
  console.log(`agent_eval_summary=${path.relative(workspaceRoot, output)}`);
  process.exit(0);
}

if (mode !== "smoke" && mode !== "full") {
  console.error(
    "usage: node scripts/agent-eval.mjs <smoke|full|live preflight|live pilot --approve profile-id>",
  );
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
