import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  closeSync,
  existsSync,
  fsyncSync,
  mkdirSync,
  openSync,
  readFileSync,
  realpathSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
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
  // Live provider exits may require the same proxy the desktop app follows.
  "HTTP_PROXY",
  "HTTPS_PROXY",
  "ALL_PROXY",
  "NO_PROXY",
  "http_proxy",
  "https_proxy",
  "all_proxy",
  "no_proxy",
  "SOCKS_PROXY",
  "SOCKS5_PROXY",
  "socks_proxy",
  "socks5_proxy",
]);
const allowedControlKeys = new Set([
  "IRIS_AGENT_EVAL_MODE",
  "IRIS_AGENT_EVAL_LIVE_ACTION",
  "IRIS_AGENT_EVAL_SOURCE_DB",
  "IRIS_AGENT_EVAL_SESSION",
  "IRIS_AGENT_EVAL_APPROVED_PROFILE",
  "IRIS_AGENT_EVAL_COST_CONFIRMATION",
  "IRIS_AGENT_EVAL_CREDENTIAL_PROBE",
  "IRIS_AGENT_EVAL_DIRECT_HTTPS",
  "IRIS_AGENT_EVAL_MODEL_ALLOWLIST",
  "IRIS_AGENT_EVAL_LIVE_RESULT_TO_VALIDATE",
  "IRIS_AGENT_EVAL_LIVE_RESULT_SHA256",
  "IRIS_DATA_DIR",
  "IRIS_CONFIG_DIR",
]);

export function buildAgentEvalChildEnvironment(source, controls = {}) {
  const environment = {};
  for (const [key, value] of Object.entries(source)) {
    if (allowedEnvironmentKeys.has(key) && typeof value === "string") {
      if (key.toUpperCase().includes("PROXY") && /:\/\/[^/@]+@/.test(value)) {
        // Proxy endpoints may be inherited for live HTTPS exits, but embedded
        // proxy credentials must never cross into the evaluation subprocess.
        continue;
      }
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

export function assertStrictSmokeSummary(summary) {
  if (!summary || typeof summary !== "object") {
    throw new Error("agent_eval_smoke_summary_invalid");
  }
  if (
    summary.caseCount !== 24 ||
    summary.completedCaseCount !== 24 ||
    summary.completedCaseCount !== summary.caseCount
  ) {
    throw new Error("agent_eval_smoke_incomplete");
  }
  if (summary.passed !== 24 || summary.failed !== 0) {
    throw new Error("agent_eval_smoke_failed");
  }
}

export function assertStrictContractSummary(summary) {
  if (!summary || typeof summary !== "object") {
    throw new Error("agent_eval_contract_summary_invalid");
  }
  if (summary.schemaVersion !== "agent-eval-summary-v2") {
    throw new Error("agent_eval_contract_summary_invalid");
  }
  if (summary.caseCount !== 48 || summary.executedCaseCount !== 48) {
    throw new Error("agent_eval_contract_incomplete");
  }
  if (
    summary.answeredCaseCount +
      summary.expectedRefusalCount +
      summary.unexpectedFailureCount !==
      48 ||
    summary.completedCaseCount !== summary.answeredCaseCount ||
    summary.passed !== 48 ||
    summary.failed !== 0 ||
    summary.unexpectedFailureCount !== 0
  ) {
    throw new Error("agent_eval_contract_failed");
  }
}

export function hasUnsafeCredentialMetadata(
  metadata,
  runtimePlatform = process.platform,
  currentUid = typeof process.getuid === "function"
    ? process.getuid()
    : undefined,
) {
  if (runtimePlatform === "win32") {
    return false;
  }
  const wrongOwner = currentUid !== undefined && metadata.uid !== currentUid;
  return wrongOwner || (metadata.mode & 0o022) !== 0;
}

function canonicalCredentialRoot(raw) {
  if (typeof raw !== "string" || raw.length === 0) {
    throw new Error("agent_eval_live_credential_roots_required");
  }
  if (!path.isAbsolute(raw)) {
    throw new Error("agent_eval_live_credential_root_invalid");
  }
  let canonical;
  let metadata;
  try {
    canonical = realpathSync(raw);
    metadata = statSync(canonical);
  } catch {
    throw new Error("agent_eval_live_credential_root_invalid");
  }
  const filesystemRoot = path.parse(canonical).root;
  if (
    !metadata.isDirectory() ||
    canonical === filesystemRoot ||
    hasUnsafeCredentialMetadata(metadata)
  ) {
    throw new Error("agent_eval_live_credential_root_invalid");
  }
  return canonical;
}

function canonicalSourceDatabase(raw) {
  if (typeof raw !== "string" || raw.length === 0 || !path.isAbsolute(raw)) {
    throw new Error("agent_eval_live_source_invalid");
  }
  try {
    const canonical = realpathSync(raw);
    const metadata = statSync(canonical);
    if (!metadata.isFile() || hasUnsafeCredentialMetadata(metadata)) {
      throw new Error("agent_eval_live_source_invalid");
    }
    return canonical;
  } catch {
    throw new Error("agent_eval_live_source_invalid");
  }
}

export function resolveLiveEvaluationPaths(
  source,
  evaluationWorkspaceRoot = workspaceRoot,
) {
  const customSource =
    typeof source.IRIS_AGENT_EVAL_SOURCE_DB === "string" &&
    source.IRIS_AGENT_EVAL_SOURCE_DB.length > 0;
  if (customSource && (!source.IRIS_DATA_DIR || !source.IRIS_CONFIG_DIR)) {
    throw new Error("agent_eval_live_custom_roots_required");
  }
  const dataDir = canonicalCredentialRoot(
    source.IRIS_DATA_DIR ||
      path.join(evaluationWorkspaceRoot, ".iris-dev", "app-data"),
  );
  const configDir = canonicalCredentialRoot(
    source.IRIS_CONFIG_DIR ||
      path.join(evaluationWorkspaceRoot, ".iris-dev", "config"),
  );
  const sourceDatabase = canonicalSourceDatabase(
    customSource
      ? source.IRIS_AGENT_EVAL_SOURCE_DB
      : path.join(dataDir, "iris.db"),
  );
  let boundDatabase;
  try {
    boundDatabase = realpathSync(path.join(dataDir, "iris.db"));
  } catch {
    throw new Error("agent_eval_live_source_root_mismatch");
  }
  if (sourceDatabase !== boundDatabase) {
    throw new Error("agent_eval_live_source_root_mismatch");
  }
  return { sourceDatabase, dataDir, configDir };
}

export function buildLivePilotChildEnvironment(
  source,
  controls = {},
  resolvedPaths = resolveLiveEvaluationPaths(source),
) {
  return buildAgentEvalChildEnvironment(source, {
    ...controls,
    IRIS_AGENT_EVAL_SOURCE_DB: resolvedPaths.sourceDatabase,
    IRIS_DATA_DIR: resolvedPaths.dataDir,
    IRIS_CONFIG_DIR: resolvedPaths.configDir,
  });
}

function runCargoEntrypoint(testName, controls, environmentBuilder) {
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
      env: environmentBuilder(process.env, controls),
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

function approvedModelAllowlist() {
  const value = argumentValue("--models");
  if (value === undefined) return undefined;
  if (
    !/^[A-Za-z0-9._-]+(?:,[A-Za-z0-9._-]+)*$/.test(value) ||
    value.split(",").length > 8
  ) {
    console.error("agent_eval_live_models_invalid");
    process.exit(2);
  }
  return value;
}

function runLive() {
  const action = process.argv[3];
  const resolvePathsOrExit = () => {
    try {
      return resolveLiveEvaluationPaths(process.env);
    } catch (error) {
      console.error(
        error instanceof Error
          ? error.message
          : "agent_eval_live_path_resolution_failed",
      );
      process.exit(2);
    }
  };
  if (action === "campaign") {
    const session = argumentValue("--session");
    const approvedProfiles = argumentValue("--approve");
    const costConfirmation = argumentValue("--confirm-cost");
    const modelAllowlist = approvedModelAllowlist();
    if (!session || !/^session-[0-9a-f]{64}$/.test(session)) {
      console.error("agent_eval_live_requires_current_session");
      process.exit(2);
    }
    const profiles = approvedProfiles?.split(",") ?? [];
    if (
      profiles.length !== 2 ||
      profiles[0] === profiles[1] ||
      profiles.some((profile) => !/^profile-[0-9a-f]{32}$/.test(profile))
    ) {
      console.error("agent_eval_live_requires_two_distinct_approved_profiles");
      process.exit(2);
    }
    if (costConfirmation !== "two-route-12-run-campaign") {
      console.error("agent_eval_live_campaign_requires_user_cost_checkpoint");
      process.exit(2);
    }
    const resolvedPaths = resolvePathsOrExit();
    const outputDirectory = path.join(workspaceRoot, "target", "agent-eval");
    const outputs = ["a", "b"].map((route) =>
      path.join(outputDirectory, `live-pilot-${session}-route-${route}.json`),
    );
    const packets = ["a", "b"].map((route) =>
      path.join(outputDirectory, `live-review-${session}-route-${route}.json`),
    );
    for (const output of [...outputs, ...packets]) {
      if (existsSync(output) || existsSync(`${output}.attestation.json`)) {
        console.error("agent_eval_live_campaign_artifact_already_exists");
        process.exit(2);
      }
    }
    const result = runCargoEntrypoint(
      "ai_runtime::agent_capacity_eval_tests::live_campaign_command_entrypoint_runs_two_explicit_routes_when_requested",
      {
        IRIS_AGENT_EVAL_LIVE_ACTION: "campaign",
        IRIS_AGENT_EVAL_SOURCE_DB: resolvedPaths.sourceDatabase,
        IRIS_AGENT_EVAL_SESSION: session,
        IRIS_AGENT_EVAL_APPROVED_PROFILE: approvedProfiles,
        IRIS_AGENT_EVAL_COST_CONFIRMATION: costConfirmation,
        ...(modelAllowlist
          ? { IRIS_AGENT_EVAL_MODEL_ALLOWLIST: modelAllowlist }
          : {}),
      },
      (source, controls) =>
        buildLivePilotChildEnvironment(source, controls, resolvedPaths),
    );
    if (result.error) {
      console.error("agent_eval_live_campaign_runner_failed");
      process.exit(1);
    }
    for (const output of outputs) {
      if (!existsSync(output)) {
        console.error("agent_eval_live_campaign_summary_missing");
        process.exit(result.status ?? 1);
      }
      console.log(`agent_eval_summary=${path.relative(workspaceRoot, output)}`);
    }
    if (result.status !== 0) {
      process.exit(result.status);
    }
    return;
  }
  if (action === "pilot") {
    const session = argumentValue("--session");
    const approvedProfile = argumentValue("--approve");
    const costConfirmation = argumentValue("--confirm-cost");
    const modelAllowlist = approvedModelAllowlist();
    if (!session || !/^session-[0-9a-f]{64}$/.test(session)) {
      console.error("agent_eval_live_requires_current_session");
      process.exit(2);
    }
    if (!approvedProfile || !/^profile-[0-9a-f]{32}$/.test(approvedProfile)) {
      console.error("agent_eval_live_requires_an_explicit_approved_profile");
      process.exit(2);
    }
    if (costConfirmation !== "one-6-case-interaction-matrix-pilot") {
      console.error("agent_eval_live_pilot_requires_user_cost_checkpoint");
      process.exit(2);
    }
    const resolvedPaths = resolvePathsOrExit();
    const sourceDatabase = resolvedPaths.sourceDatabase;
    const result = runCargoEntrypoint(
      "ai_runtime::agent_capacity_eval_tests::live_pilot_command_entrypoint_runs_only_an_approved_current_session_when_requested",
      {
        IRIS_AGENT_EVAL_LIVE_ACTION: "pilot",
        IRIS_AGENT_EVAL_SOURCE_DB: sourceDatabase,
        IRIS_AGENT_EVAL_SESSION: session,
        IRIS_AGENT_EVAL_APPROVED_PROFILE: approvedProfile,
        IRIS_AGENT_EVAL_COST_CONFIRMATION: costConfirmation,
        ...(modelAllowlist
          ? { IRIS_AGENT_EVAL_MODEL_ALLOWLIST: modelAllowlist }
          : {}),
      },
      (source, controls) =>
        buildLivePilotChildEnvironment(source, controls, resolvedPaths),
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

  const modelAllowlist = approvedModelAllowlist();
  if (
    action !== "preflight" ||
    (process.argv.length !== 4 &&
      !(process.argv.length === 6 && modelAllowlist !== undefined))
  ) {
    console.error(
      "agent_eval_live_requires_preflight_or_an_explicit_approved_profile",
    );
    process.exit(2);
  }
  const resolvedPaths = resolvePathsOrExit();
  const sourceDatabase = resolvedPaths.sourceDatabase;
  const output = path.join(
    workspaceRoot,
    "target",
    "agent-eval",
    "live-preflight.json",
  );
  rmSync(output, { force: true });
  const result = runCargoEntrypoint(
    "ai_runtime::agent_capacity_eval_tests::live_preflight_command_entrypoint_writes_only_the_anonymous_report_when_requested",
    {
      IRIS_AGENT_EVAL_LIVE_ACTION: "preflight",
      IRIS_AGENT_EVAL_SOURCE_DB: sourceDatabase,
      ...(modelAllowlist
        ? { IRIS_AGENT_EVAL_MODEL_ALLOWLIST: modelAllowlist }
        : {}),
    },
    (source, controls) =>
      buildLivePilotChildEnvironment(source, controls, resolvedPaths),
  );
  exitFromCargo(result, "agent_eval_live_preflight_runner_failed");
  if (!existsSync(output)) {
    console.error("agent_eval_live_preflight_summary_missing");
    process.exit(1);
  }
  console.log(`agent_eval_summary=${path.relative(workspaceRoot, output)}`);
}

function runProductGate() {
  const outputDirectory = path.join(workspaceRoot, "target", "agent-eval");
  const output = path.join(outputDirectory, "product-gate.json");
  mkdirSync(outputDirectory, { recursive: true });
  // A failed invocation must never leave a previously passed gate visible.
  rmSync(output, { force: true });
  const liveResults = process.env.IRIS_AGENT_EVAL_LIVE_RESULTS;
  const liveReview = process.env.IRIS_AGENT_EVAL_LIVE_REVIEW;
  const requestedPaths = liveResults
    ?.split(path.delimiter)
    .map((value) => value.trim())
    .filter(Boolean);
  if (
    requestedPaths?.length !== 2 ||
    requestedPaths.some((value) => !path.isAbsolute(value))
  ) {
    console.error("agent_eval_two_live_results_required");
    process.exit(2);
  }
  if (!liveReview || !path.isAbsolute(liveReview)) {
    console.error("agent_eval_live_review_required");
    process.exit(2);
  }
  let canonicalResults;
  let reportSnapshots;
  let reports;
  let canonicalReview;
  let review;
  try {
    const resolvedPaths = resolveLiveEvaluationPaths(process.env);
    const targetRoot = realpathSync(
      path.join(workspaceRoot, "target", "agent-eval"),
    );
    canonicalResults = requestedPaths.map((requested) => {
      const canonical = realpathSync(requested);
      if (!canonical.startsWith(`${targetRoot}${path.sep}`)) {
        throw new Error("agent_eval_live_result_outside_target");
      }
      const metadata = statSync(canonical);
      if (!metadata.isFile() || metadata.size > 256 * 1024) {
        throw new Error("agent_eval_live_result_invalid");
      }
      return canonical;
    });
    if (new Set(canonicalResults).size !== 2) {
      throw new Error("agent_eval_live_routes_must_be_distinct");
    }
    reportSnapshots = canonicalResults.map((canonical) =>
      readFileSync(canonical),
    );
    for (let index = 0; index < canonicalResults.length; index += 1) {
      const canonical = canonicalResults[index];
      const snapshotHash = createHash("sha256")
        .update(reportSnapshots[index])
        .digest("hex");
      const validation = runCargoEntrypoint(
        "ai_runtime::agent_capacity_eval_tests::live_result_validation_command_entrypoint_rejects_tampered_artifacts",
        {
          IRIS_AGENT_EVAL_LIVE_RESULT_TO_VALIDATE: canonical,
          IRIS_AGENT_EVAL_LIVE_RESULT_SHA256: snapshotHash,
          IRIS_CONFIG_DIR: resolvedPaths.configDir,
        },
        buildAgentEvalChildEnvironment,
      );
      if (validation.error || validation.status !== 0) {
        throw new Error("agent_eval_live_result_strict_validation_failed");
      }
    }
    reports = reportSnapshots.map((snapshot) =>
      JSON.parse(snapshot.toString("utf8")),
    );
    canonicalReview = realpathSync(liveReview);
    if (!canonicalReview.startsWith(`${targetRoot}${path.sep}`)) {
      throw new Error("agent_eval_live_review_outside_target");
    }
    const reviewMetadata = statSync(canonicalReview);
    if (!reviewMetadata.isFile() || reviewMetadata.size > 128 * 1024) {
      throw new Error("agent_eval_live_review_invalid");
    }
    review = JSON.parse(readFileSync(canonicalReview, "utf8"));
    validateProductQualityArtifacts(
      reports,
      review,
      canonicalResults.map((canonical) => path.basename(canonical)),
    );
  } catch (error) {
    console.error(
      error instanceof Error
        ? error.message
        : "agent_eval_live_artifact_invalid",
    );
    process.exit(2);
  }
  const contract = spawnSync(process.execPath, [scriptPath, "full"], {
    cwd: workspaceRoot,
    env: buildAgentEvalChildEnvironment(process.env),
    stdio: "inherit",
  });
  if (contract.error || contract.status !== 0) {
    console.error("agent_eval_contract_gate_failed");
    process.exit(contract.status ?? 1);
  }
  const totalLiveRuns = reports.reduce(
    (total, report) => total + report.caseCount,
    0,
  );
  const productReport = {
    schemaVersion: "agent-product-gate-v1",
    status: "product_gate_passed",
    liveRunCount: totalLiveRuns,
    liveRouteCount: reports.length,
    dimensions: {
      contract: "passed",
      safety: "passed",
      continuity: "passed",
      loopTrace: "passed",
      source: "passed",
      realAnswerQuality: "passed_with_human_review",
    },
    contractSummary: "core-full.json",
    liveSummaries: canonicalResults.map((canonical) =>
      path.basename(canonical),
    ),
    liveReview: path.basename(canonicalReview),
  };
  const temporaryOutput = `${output}.${process.pid}.tmp`;
  rmSync(temporaryOutput, { force: true });
  try {
    const descriptor = openSync(temporaryOutput, "wx", 0o600);
    try {
      writeFileSync(
        descriptor,
        `${JSON.stringify(productReport, null, 2)}\n`,
        "utf8",
      );
      fsyncSync(descriptor);
    } finally {
      closeSync(descriptor);
    }
    renameSync(temporaryOutput, output);
  } catch {
    rmSync(temporaryOutput, { force: true });
    console.error("agent_eval_product_report_write_failed");
    process.exit(1);
  }
  console.log(`agent_eval_summary=${path.relative(workspaceRoot, output)}`);
}

export function validateProductQualityArtifacts(reports, review, reportNames) {
  if (
    !Array.isArray(reports) ||
    reports.length !== 2 ||
    !Array.isArray(reportNames) ||
    reportNames.length !== 2 ||
    new Set(reportNames).size !== 2
  ) {
    throw new Error("agent_eval_two_live_routes_required");
  }
  let totalRuns = 0;
  let totalModelCalls = 0;
  let totalWebCalls = 0;
  const expectedReviews = new Set();
  const packetHashes = new Map();
  const routeCommitments = new Set();
  const campaignIds = new Set();
  const routeLabels = new Set();
  const requiredCaseIds = new Set([1, 26, 28, 30, 32, 34]);
  const campaignBudgets = [];
  for (let index = 0; index < reports.length; index += 1) {
    const report = reports[index];
    if (
      report?.schemaVersion !== "agent-live-pilot-v3" ||
      typeof report.routeCommitment !== "string" ||
      !/^route-[a-f0-9]{64}$/.test(report.routeCommitment) ||
      !["Route A", "Route B"].includes(report.routeLabel) ||
      typeof report.campaignId !== "string" ||
      !/^campaign-[a-f0-9]{64}$/.test(report.campaignId) ||
      report?.status !== "live_trace_executed" ||
      report.caseCount !== 6 ||
      report.requiredCaseCount !== 6 ||
      report.completedCaseCount !== 6 ||
      report.mechanicalPassed !== 6 ||
      report.mechanicalFailed !== 0 ||
      typeof report.reviewPacketSha256 !== "string" ||
      !/^[a-f0-9]{64}$/.test(report.reviewPacketSha256) ||
      report.campaignBudget?.maxRuns !== 12 ||
      report.campaignBudget?.maxModelTurns !== 48 ||
      report.campaignBudget?.maxWebToolCalls !== 36 ||
      !Array.isArray(report.cases) ||
      report.cases.length !== 6
    ) {
      throw new Error("agent_eval_live_quality_gate_failed");
    }
    packetHashes.set(reportNames[index], report.reviewPacketSha256);
    routeCommitments.add(report.routeCommitment);
    campaignIds.add(report.campaignId);
    routeLabels.add(report.routeLabel);
    campaignBudgets.push(report.campaignBudget);
    totalRuns += report.caseCount;
    const observedCaseIds = new Set();
    let observedCompleted = 0;
    let observedPassed = 0;
    for (const item of report.cases) {
      const requiresWeb = item.caseId !== 1;
      if (
        !Number.isInteger(item?.caseId) ||
        !requiredCaseIds.has(item.caseId) ||
        observedCaseIds.has(item.caseId) ||
        item.repetition !== 1 ||
        item.semanticStatus !== "pending_human_review" ||
        item.mechanical?.terminal !== "pass" ||
        item.mechanical?.authorization !== "pass" ||
        item.mechanical?.safety !== "pass" ||
        (item.mechanical?.continuity !== "pass" &&
          !(
            item.caseId === 1 &&
            item.mechanical?.continuity === "not_applicable"
          )) ||
        (requiresWeb &&
          (item.mechanical?.searchFetchTrace !== "pass" ||
            item.mechanical?.runLocalSources !== "pass" ||
            item.mechanical?.citationBinding !== "pass")) ||
        (!requiresWeb &&
          (item.mechanical?.searchFetchTrace !== "not_applicable" ||
            item.mechanical?.runLocalSources !== "not_applicable" ||
            item.mechanical?.citationBinding !== "not_applicable")) ||
        !Number.isInteger(item.telemetry?.modelTurns) ||
        !Number.isInteger(item.telemetry?.toolCalls)
      ) {
        throw new Error("agent_eval_live_case_identity_invalid");
      }
      observedCaseIds.add(item.caseId);
      observedCompleted += 1;
      observedPassed += 1;
      totalModelCalls += item.telemetry.modelTurns;
      totalWebCalls += item.telemetry.toolCalls;
      if (requiresWeb && item.telemetry.toolCalls < 2) {
        throw new Error("agent_eval_live_loop_or_source_contract_failed");
      }
      expectedReviews.add(`${reportNames[index]}:${item.caseId}`);
    }
    if (
      observedCaseIds.size !== requiredCaseIds.size ||
      observedCompleted !== report.completedCaseCount ||
      observedPassed !== report.mechanicalPassed ||
      report.mechanicalFailed !== report.caseCount - observedPassed
    ) {
      throw new Error("agent_eval_live_count_inconsistent");
    }
  }
  if (routeCommitments.size !== 2 || routeLabels.size !== 2) {
    throw new Error("agent_eval_live_routes_must_be_distinct");
  }
  if (campaignIds.size !== 1) {
    throw new Error("agent_eval_live_campaign_mismatch");
  }
  if (totalRuns !== 12 || expectedReviews.size !== totalRuns) {
    throw new Error("agent_eval_live_run_budget_invalid");
  }
  if (totalModelCalls > 48 || totalWebCalls > 36) {
    throw new Error("agent_eval_live_call_budget_invalid");
  }
  if (
    campaignBudgets.some(
      (budget) =>
        budget.observedRuns !== totalRuns ||
        budget.observedModelTurns !== totalModelCalls ||
        budget.observedWebToolCalls !== totalWebCalls,
    )
  ) {
    throw new Error("agent_eval_live_campaign_budget_inconsistent");
  }
  if (
    review?.schemaVersion !== "agent-live-review-v2" ||
    review.status !== "approved" ||
    !Array.isArray(review.items) ||
    review.items.length !== totalRuns
  ) {
    throw new Error("agent_eval_live_review_invalid");
  }
  const seen = new Set();
  let scoreTotal = 0;
  let scoreCount = 0;
  for (const item of review.items) {
    const key = `${item?.report}:${item?.caseId}`;
    if (!expectedReviews.has(key) || seen.has(key)) {
      throw new Error("agent_eval_live_review_case_invalid");
    }
    if (
      item?.reviewPacketSha256 !== packetHashes.get(item.report) ||
      item?.directFailure !== false
    ) {
      throw new Error("agent_eval_live_review_packet_mismatch");
    }
    seen.add(key);
    for (const field of [
      "intentFollowing",
      "factualSources",
      "relevanceCompleteness",
      "correctionContinuity",
    ]) {
      const score = item?.[field];
      if (!Number.isFinite(score) || score < 4 || score > 5) {
        throw new Error("agent_eval_live_review_score_invalid");
      }
      scoreTotal += score;
      scoreCount += 1;
    }
  }
  if (seen.size !== expectedReviews.size || scoreTotal / scoreCount < 4.2) {
    throw new Error("agent_eval_live_review_threshold_not_met");
  }
  return { totalRuns, averageScore: scoreTotal / scoreCount };
}

function main() {
  const mode = process.argv[2];
  if (mode === "live") {
    runLive();
    return;
  }
  if (mode === "gate") {
    runProductGate();
    return;
  }
  if (mode !== "smoke" && mode !== "full") {
    console.error(
      "usage: node scripts/agent-eval.mjs <smoke|full|gate|live preflight [--models model-a,model-b]|live campaign --session session-id --approve profile-a,profile-b --confirm-cost two-route-12-run-campaign [--models model-a,model-b]>",
    );
    process.exit(2);
  }
  const output = path.join(
    workspaceRoot,
    "target",
    "agent-eval",
    mode === "smoke" ? "core-smoke.json" : "core-full.json",
  );
  // Do not let a failed subprocess be mistaken for a fresh evaluation merely
  // because a report from an earlier run remains on disk.
  rmSync(output, { force: true });
  const result = runCargoEntrypoint(
    "ai_runtime::agent_capacity_eval_tests::deterministic_command_entrypoint_writes_only_the_strict_summary_when_requested",
    { IRIS_AGENT_EVAL_MODE: mode },
    buildAgentEvalChildEnvironment,
  );
  exitFromCargo(result, "agent_eval_runner_failed");
  if (!existsSync(output)) {
    console.error("agent_eval_summary_missing");
    process.exit(1);
  }
  if (mode === "smoke" || mode === "full") {
    try {
      const summary = JSON.parse(readFileSync(output, "utf8"));
      if (mode === "smoke") {
        assertStrictSmokeSummary(summary);
      } else {
        assertStrictContractSummary(summary);
      }
    } catch (error) {
      console.error(
        error instanceof Error
          ? error.message
          : mode === "smoke"
            ? "agent_eval_smoke_summary_invalid"
            : "agent_eval_contract_summary_invalid",
      );
      process.exit(1);
    }
  }
  console.log(`agent_eval_summary=${path.relative(workspaceRoot, output)}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === scriptPath) {
  main();
}
