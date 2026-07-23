import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const scriptPath = fileURLToPath(import.meta.url);
const workspaceRoot = path.resolve(path.dirname(scriptPath), "..");
const allowedEnvironmentKeys = new Set([
  "PATH",
  "HOME",
  "USER",
  "LOGNAME",
  "SHELL",
  "TMPDIR",
  "TMP",
  "TEMP",
  "LANG",
  "LC_ALL",
  "LC_CTYPE",
  "TERM",
  "NO_COLOR",
  "CI",
  "CARGO_HOME",
  "RUSTUP_HOME",
  "CARGO_TARGET_DIR",
  "RUSTC",
  "RUSTDOC",
  "RUSTFLAGS",
  "RUST_BACKTRACE",
  "SDKROOT",
  "MACOSX_DEPLOYMENT_TARGET",
  "PKG_CONFIG_PATH",
  "LD_LIBRARY_PATH",
  "DYLD_LIBRARY_PATH",
]);
const allowedControlKeys = new Set([
  "IRIS_AGENT_EVAL_MODE",
  "IRIS_AGENT_EVAL_LIVE_ACTION",
  "IRIS_AGENT_EVAL_SOURCE_DB",
  "IRIS_AGENT_EVAL_SESSION",
  "IRIS_AGENT_EVAL_APPROVED_PROFILE",
  "IRIS_AGENT_EVAL_COST_CONFIRMATION",
]);

export function buildAgentEvalChildEnvironment(source, controls = {}) {
  const environment = {};
  for (const [key, value] of Object.entries(source)) {
    if (allowedEnvironmentKeys.has(key) && typeof value === "string") {
      environment[key] = value;
    }
  }
  for (const [key, value] of Object.entries(controls)) {
    if (allowedControlKeys.has(key) && typeof value === "string") {
      environment[key] = value;
    }
  }
  return environment;
}

function runCargoEntrypoint(testName, controls) {
  return spawnSync(
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
      env: buildAgentEvalChildEnvironment(process.env, controls),
      stdio: "inherit",
    },
  );
}

function exitFromCargo(result, failureCode) {
  if (result.error) {
    console.error(failureCode);
    process.exit(1);
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function argumentValue(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

function runLive() {
  const action = process.argv[3];
  const sourceDatabase =
    process.env.IRIS_AGENT_EVAL_SOURCE_DB ||
    path.join(
      process.env.IRIS_DATA_DIR ||
        path.join(workspaceRoot, ".iris-dev", "app-data"),
      "iris.db",
    );
  if (action === "pilot") {
    const session = argumentValue("--session");
    const approvedProfile = argumentValue("--approve");
    const costConfirmation = argumentValue("--confirm-cost");
    if (!session || !/^session-[0-9a-f]{64}$/.test(session)) {
      console.error("agent_eval_live_requires_current_session");
      process.exit(2);
    }
    if (!approvedProfile || !/^profile-[0-9a-f]{32}$/.test(approvedProfile)) {
      console.error("agent_eval_live_requires_an_explicit_approved_profile");
      process.exit(2);
    }
    if (costConfirmation !== "one-12-case-pilot") {
      console.error("agent_eval_live_pilot_requires_user_cost_checkpoint");
      process.exit(2);
    }
    if (!existsSync(sourceDatabase)) {
      console.error("agent_eval_live_pilot_source_missing");
      process.exit(2);
    }
    const result = runCargoEntrypoint(
      "ai_runtime::agent_capacity_eval_tests::live_pilot_command_entrypoint_runs_only_an_approved_current_session_when_requested",
      {
        IRIS_AGENT_EVAL_LIVE_ACTION: "pilot",
        IRIS_AGENT_EVAL_SOURCE_DB: sourceDatabase,
        IRIS_AGENT_EVAL_SESSION: session,
        IRIS_AGENT_EVAL_APPROVED_PROFILE: approvedProfile,
        IRIS_AGENT_EVAL_COST_CONFIRMATION: costConfirmation,
      },
    );
    exitFromCargo(result, "agent_eval_live_pilot_runner_failed");
    const output = path.join(
      workspaceRoot,
      "target",
      "agent-eval",
      `live-pilot-${session}.json`,
    );
    if (!existsSync(output)) {
      console.error("agent_eval_live_pilot_summary_missing");
      process.exit(1);
    }
    console.log(`agent_eval_summary=${path.relative(workspaceRoot, output)}`);
    return;
  }

  if (action !== "preflight" || process.argv.length !== 4) {
    console.error(
      "agent_eval_live_requires_preflight_or_an_explicit_approved_profile",
    );
    process.exit(2);
  }
  if (!existsSync(sourceDatabase)) {
    console.error("agent_eval_live_preflight_source_missing");
    process.exit(2);
  }
  const result = runCargoEntrypoint(
    "ai_runtime::agent_capacity_eval_tests::live_preflight_command_entrypoint_writes_only_the_anonymous_report_when_requested",
    {
      IRIS_AGENT_EVAL_LIVE_ACTION: "preflight",
      IRIS_AGENT_EVAL_SOURCE_DB: sourceDatabase,
    },
  );
  exitFromCargo(result, "agent_eval_live_preflight_runner_failed");
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
}

function main() {
  const mode = process.argv[2];
  if (mode === "live") {
    runLive();
    return;
  }
  if (mode !== "smoke" && mode !== "full") {
    console.error(
      "usage: node scripts/agent-eval.mjs <smoke|full|live preflight|live pilot --session session-id --approve profile-id --confirm-cost one-12-case-pilot>",
    );
    process.exit(2);
  }
  const result = runCargoEntrypoint(
    "ai_runtime::agent_capacity_eval_tests::deterministic_command_entrypoint_writes_only_the_strict_summary_when_requested",
    {
      IRIS_AGENT_EVAL_MODE: mode,
    },
  );
  exitFromCargo(result, "agent_eval_runner_failed");
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
}

if (process.argv[1] && path.resolve(process.argv[1]) === scriptPath) {
  main();
}
