import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  existsSync,
  mkdtempSync,
  mkdirSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  buildAgentEvalChildEnvironment,
  buildLivePilotChildEnvironment,
  resolveLiveEvaluationPaths,
} from "./agent-eval.mjs";

const workspaceRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);

test("agent eval child receives only the explicit toolchain allowlist", () => {
  const environment = buildAgentEvalChildEnvironment(
    {
      PATH: process.env.PATH ?? "/usr/bin:/bin",
      LANG: "zh_CN.UTF-8",
      PRIVATE_KEY: "private-key-must-not-cross",
      AWS_ACCESS_KEY_ID: "aws-access-key-must-not-cross",
      DATABASE_URL: "postgres://user:password@private.invalid/database",
      HTTP_PROXY: "http://proxy-user:proxy-password@proxy.invalid:8080",
      HTTPS_PROXY: "https://proxy-user:proxy-password@proxy.invalid:8443",
      ANYSEARCH_API_KEY: "anysearch-key-must-not-cross",
      MINIMAX_API_KEY: "minimax-key-must-not-cross",
    },
    {
      IRIS_AGENT_EVAL_MODE: "smoke",
    },
  );
  const child = spawnSync(
    process.execPath,
    ["-e", "process.stdout.write(JSON.stringify(process.env))"],
    {
      env: environment,
      encoding: "utf8",
    },
  );

  assert.equal(child.status, 0, child.stderr);
  const captured = JSON.parse(child.stdout);
  assert.equal(captured.PATH, environment.PATH);
  assert.equal(captured.LANG, "zh_CN.UTF-8");
  assert.equal(captured.IRIS_AGENT_EVAL_MODE, "smoke");
  for (const forbidden of [
    "PRIVATE_KEY",
    "AWS_ACCESS_KEY_ID",
    "DATABASE_URL",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ANYSEARCH_API_KEY",
    "MINIMAX_API_KEY",
  ]) {
    assert.equal(captured[forbidden], undefined, forbidden);
  }
  assert.doesNotMatch(
    child.stdout,
    /private-key|aws-access|password|anysearch-key|minimax-key/,
  );
});

test("live pilot CLI requires session, profile and exact one-run cost confirmation", () => {
  const run = (...args) =>
    spawnSync(
      process.execPath,
      [
        path.join(workspaceRoot, "scripts/agent-eval.mjs"),
        "live",
        "pilot",
        ...args,
      ],
      {
        cwd: workspaceRoot,
        env: {
          ...process.env,
          IRIS_AGENT_EVAL_SOURCE_DB: path.join(
            workspaceRoot,
            "target/agent-eval/definitely-missing.db",
          ),
        },
        encoding: "utf8",
      },
    );
  const session = `session-${"a".repeat(64)}`;
  const profile = `profile-${"b".repeat(32)}`;

  assert.match(run().stderr, /agent_eval_live_requires_current_session/);
  assert.match(
    run("--session", session).stderr,
    /agent_eval_live_requires_an_explicit_approved_profile/,
  );
  assert.match(
    run("--session", session, "--approve", profile).stderr,
    /agent_eval_live_pilot_requires_user_cost_checkpoint/,
  );
  assert.match(
    run(
      "--session",
      session,
      "--approve",
      profile,
      "--confirm-cost",
      "one-12-case-pilot",
    ).stderr,
    /agent_eval_live_custom_roots_required/,
  );
});

test("default live paths bind one canonical source database data root and config root", () => {
  const temporaryRoot = mkdtempSync(
    path.join(os.tmpdir(), "iris-agent-eval-default-paths-"),
  );
  const dataDir = path.join(temporaryRoot, ".iris-dev", "app-data");
  const configDir = path.join(temporaryRoot, ".iris-dev", "config");
  const sourceDatabase = path.join(dataDir, "iris.db");
  mkdirSync(dataDir, { recursive: true });
  mkdirSync(configDir, { recursive: true });
  writeFileSync(sourceDatabase, "synthetic sqlite placeholder");

  try {
    const resolved = resolveLiveEvaluationPaths(
      { PATH: process.env.PATH ?? "/usr/bin:/bin" },
      temporaryRoot,
    );
    assert.deepEqual(resolved, {
      sourceDatabase: realpathSync(sourceDatabase),
      dataDir: realpathSync(dataDir),
      configDir: realpathSync(configDir),
    });
  } finally {
    rmSync(temporaryRoot, { recursive: true, force: true });
  }
});

test("live child receives resolved roots but never credential values", () => {
  const temporaryRoot = mkdtempSync(
    path.join(os.tmpdir(), "iris-agent-eval-roots-"),
  );
  const dataDir = path.join(temporaryRoot, "data");
  const configDir = path.join(temporaryRoot, "config");
  const sourceDatabase = path.join(dataDir, "iris.db");
  mkdirSync(dataDir);
  mkdirSync(configDir);
  writeFileSync(sourceDatabase, "synthetic sqlite placeholder");
  const source = {
    PATH: process.env.PATH ?? "/usr/bin:/bin",
    IRIS_DATA_DIR: dataDir,
    IRIS_CONFIG_DIR: configDir,
    IRIS_AGENT_EVAL_SOURCE_DB: sourceDatabase,
    MINIMAX_API_KEY: "minimax-secret-must-not-cross",
    ANYSEARCH_API_KEY: "anysearch-secret-must-not-cross",
    DATABASE_URL: "postgres://user:password@private.invalid/database",
    HTTPS_PROXY: "https://proxy-user:proxy-password@proxy.invalid:8443",
  };

  try {
    const resolved = resolveLiveEvaluationPaths(source, temporaryRoot);
    const environment = buildLivePilotChildEnvironment(
      source,
      {
        IRIS_AGENT_EVAL_LIVE_ACTION: "pilot",
        IRIS_AGENT_EVAL_SOURCE_DB: sourceDatabase,
      },
      resolved,
    );
    const child = spawnSync(
      process.execPath,
      ["-e", "process.stdout.write(JSON.stringify(process.env))"],
      {
        env: environment,
        encoding: "utf8",
      },
    );
    assert.equal(child.status, 0, child.stderr);
    const captured = JSON.parse(child.stdout);
    assert.equal(captured.IRIS_DATA_DIR, realpathSync(dataDir));
    assert.equal(captured.IRIS_CONFIG_DIR, realpathSync(configDir));
    assert.equal(
      captured.IRIS_AGENT_EVAL_SOURCE_DB,
      realpathSync(sourceDatabase),
    );
    for (const forbidden of [
      "MINIMAX_API_KEY",
      "ANYSEARCH_API_KEY",
      "DATABASE_URL",
      "HTTPS_PROXY",
    ]) {
      assert.equal(captured[forbidden], undefined, forbidden);
    }
    assert.doesNotMatch(
      child.stdout,
      /minimax-secret|anysearch-secret|password|proxy-user/,
    );
  } finally {
    rmSync(temporaryRoot, { recursive: true, force: true });
  }
});

test("live path resolution rejects missing default config and unbound custom databases", () => {
  const temporaryRoot = mkdtempSync(
    path.join(os.tmpdir(), "iris-agent-eval-invalid-roots-"),
  );
  const defaultDataDir = path.join(temporaryRoot, ".iris-dev", "app-data");
  mkdirSync(defaultDataDir, { recursive: true });
  writeFileSync(path.join(defaultDataDir, "iris.db"), "synthetic sqlite");
  const customDataDir = path.join(temporaryRoot, "custom-data");
  const otherDataDir = path.join(temporaryRoot, "other-data");
  const configDir = path.join(temporaryRoot, "config");
  mkdirSync(customDataDir);
  mkdirSync(otherDataDir);
  mkdirSync(configDir);
  const customDatabase = path.join(customDataDir, "iris.db");
  writeFileSync(customDatabase, "synthetic sqlite");

  try {
    assert.throws(
      () =>
        resolveLiveEvaluationPaths(
          { PATH: process.env.PATH ?? "/usr/bin:/bin" },
          temporaryRoot,
        ),
      /agent_eval_live_credential_root_invalid/,
    );
    assert.throws(
      () =>
        resolveLiveEvaluationPaths(
          {
            PATH: process.env.PATH ?? "/usr/bin:/bin",
            IRIS_AGENT_EVAL_SOURCE_DB: customDatabase,
          },
          temporaryRoot,
        ),
      /agent_eval_live_custom_roots_required/,
    );
    assert.throws(
      () =>
        resolveLiveEvaluationPaths(
          {
            PATH: process.env.PATH ?? "/usr/bin:/bin",
            IRIS_AGENT_EVAL_SOURCE_DB: customDatabase,
            IRIS_DATA_DIR: otherDataDir,
            IRIS_CONFIG_DIR: configDir,
          },
          temporaryRoot,
        ),
      /agent_eval_live_source_root_mismatch/,
    );
  } finally {
    rmSync(temporaryRoot, { recursive: true, force: true });
  }
});

test("live child uses the separated encrypted store without exposing selected or unselected keys", () => {
  const temporaryRoot = mkdtempSync(
    path.join(os.tmpdir(), "iris-agent-eval-encrypted-child-"),
  );
  const dataDir = path.join(temporaryRoot, "data");
  const configDir = path.join(temporaryRoot, "config");
  const sourceDatabase = path.join(dataDir, "iris.db");
  mkdirSync(dataDir);
  mkdirSync(configDir);
  writeFileSync(sourceDatabase, "");
  const source = {
    PATH: process.env.PATH ?? "/usr/bin:/bin",
    HOME: process.env.HOME,
    CARGO_HOME: process.env.CARGO_HOME,
    RUSTUP_HOME: process.env.RUSTUP_HOME,
    CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR,
    IRIS_DATA_DIR: dataDir,
    IRIS_CONFIG_DIR: configDir,
    IRIS_AGENT_EVAL_SOURCE_DB: sourceDatabase,
  };

  try {
    const resolved = resolveLiveEvaluationPaths(source, temporaryRoot);
    const environment = buildLivePilotChildEnvironment(
      source,
      {
        IRIS_AGENT_EVAL_CREDENTIAL_PROBE: "1",
        IRIS_AGENT_EVAL_SOURCE_DB: sourceDatabase,
      },
      resolved,
    );
    const child = spawnSync(
      "cargo",
      [
        "test",
        "--manifest-path",
        "src-tauri/Cargo.toml",
        "--lib",
        "ai_runtime::agent_capacity_eval_tests::approved_live_hydration_reads_only_selected_aes_gcm_credentials_and_reaches_local_transports",
        "--",
        "--exact",
        "--nocapture",
        "--test-threads=1",
      ],
      {
        cwd: workspaceRoot,
        env: environment,
        encoding: "utf8",
      },
    );
    assert.equal(child.status, 0, child.stderr);
    assert.equal(environment.IRIS_AGENT_EVAL_CREDENTIAL_PROBE, "1");
    assert.equal(
      existsSync(path.join(configDir, "master.key")),
      true,
      "separated master key must be created by the real credential backend",
    );
    assert.equal(
      existsSync(path.join(dataDir, "credentials")),
      true,
      "encrypted credential records must be created under the data root",
    );
    const output = `${child.stdout}\n${child.stderr}`;
    assert.doesNotMatch(
      output,
      /selected-llm-secret|selected-mcp-secret|unselected-secret/,
    );
  } finally {
    rmSync(temporaryRoot, { recursive: true, force: true });
  }
});
