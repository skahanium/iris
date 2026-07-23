import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
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
    /agent_eval_live_pilot_source_missing/,
  );
});

test("live pilot child receives canonical credential roots but never credential values", () => {
  const temporaryRoot = mkdtempSync(
    path.join(os.tmpdir(), "iris-agent-eval-roots-"),
  );
  const dataDir = path.join(temporaryRoot, "data");
  const configDir = path.join(temporaryRoot, "config");
  mkdirSync(dataDir);
  mkdirSync(configDir);
  const source = {
    PATH: process.env.PATH ?? "/usr/bin:/bin",
    IRIS_DATA_DIR: dataDir,
    IRIS_CONFIG_DIR: configDir,
    MINIMAX_API_KEY: "minimax-secret-must-not-cross",
    ANYSEARCH_API_KEY: "anysearch-secret-must-not-cross",
    DATABASE_URL: "postgres://user:password@private.invalid/database",
    HTTPS_PROXY: "https://proxy-user:proxy-password@proxy.invalid:8443",
  };

  try {
    const environment = buildLivePilotChildEnvironment(source, {
      IRIS_AGENT_EVAL_LIVE_ACTION: "pilot",
    });
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

test("live pilot child rejects missing relative and non-directory credential roots", () => {
  const temporaryRoot = mkdtempSync(
    path.join(os.tmpdir(), "iris-agent-eval-invalid-roots-"),
  );
  const dataDir = path.join(temporaryRoot, "data");
  const configFile = path.join(temporaryRoot, "config-file");
  mkdirSync(dataDir);
  writeFileSync(configFile, "not a directory");

  try {
    assert.throws(
      () =>
        buildLivePilotChildEnvironment(
          { PATH: process.env.PATH ?? "/usr/bin:/bin" },
          {},
        ),
      /agent_eval_live_credential_roots_required/,
    );
    assert.throws(
      () =>
        buildLivePilotChildEnvironment(
          {
            PATH: process.env.PATH ?? "/usr/bin:/bin",
            IRIS_DATA_DIR: "relative/data",
            IRIS_CONFIG_DIR: temporaryRoot,
          },
          {},
        ),
      /agent_eval_live_credential_root_invalid/,
    );
    assert.throws(
      () =>
        buildLivePilotChildEnvironment(
          {
            PATH: process.env.PATH ?? "/usr/bin:/bin",
            IRIS_DATA_DIR: dataDir,
            IRIS_CONFIG_DIR: configFile,
          },
          {},
        ),
      /agent_eval_live_credential_root_invalid/,
    );
  } finally {
    rmSync(temporaryRoot, { recursive: true, force: true });
  }
});
