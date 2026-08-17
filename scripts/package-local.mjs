#!/usr/bin/env node
import {
  cpSync,
  mkdirSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const releaseRoot = path.join(root, ".iris-dev", "target", "release");
const bundleRoot = path.join(releaseRoot, "bundle");
const appPath = path.join(bundleRoot, "macos", "Iris.app");
const updaterSigningEnabled = Boolean(process.env.TAURI_SIGNING_PRIVATE_KEY);
const embeddedModelSource = path.join(
  root,
  ".iris-dev",
  "models",
  "bge-small-zh-v1.5",
);

function usage() {
  return [
    "Usage:",
    "  node scripts/package-local.mjs [--check] mac",
    "  node scripts/package-local.mjs [--check] win",
    "",
    "Creates local self-use packages only. No Developer ID, notarization, CI, or Windows code signing.",
    "All desktop packages require the default sqlite-vec feature and pass its release smoke gate.",
  ].join("\n");
}

function parseArgs(argv) {
  const options = {
    check: false,
    target: null,
  };

  for (const arg of argv) {
    if (arg === "--check") {
      options.check = true;
      continue;
    }
    if (arg === "--help" || arg === "-h") {
      process.stdout.write(`${usage()}\n`);
      process.exit(0);
    }
    if (arg === "mac" || arg === "win") {
      options.target = arg;
      continue;
    }
    throw new Error(`Unknown argument: ${arg}`);
  }

  if (!options.target) {
    throw new Error("Missing target: expected mac or win");
  }

  return options;
}

function run(label, command, args) {
  process.stdout.write(`\n[package-local] ${label}\n`);
  const result = spawnSync(command, args, {
    cwd: root,
    shell: process.platform === "win32",
    stdio: "inherit",
    env: process.env,
  });
  if (result.status !== 0) {
    const code = result.status ?? 1;
    throw new Error(`${label} failed with exit code ${code}`);
  }
}

function packageVersion() {
  const pkg = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8"));
  return pkg.version;
}

function trustedTypesStatus() {
  const config = JSON.parse(
    readFileSync(path.join(root, "src-tauri", "tauri.conf.json"), "utf8"),
  );
  const csp = config?.app?.security?.csp ?? "";
  return csp.includes("require-trusted-types-for")
    ? "global enforcement enabled"
    : "global enforcement disabled";
}

function archLabel() {
  if (process.arch === "arm64") return "aarch64";
  if (process.arch === "x64") return "x64";
  return process.arch;
}

function tauriBuildArgs(target, configPath) {
  const args = ["run", "tauri", "--", "build", "--config", configPath];
  if (target === "mac") {
    args.push("--bundles", "app");
    return args;
  }
  args.push("--bundles", "nsis");
  return args;
}

function writePackageTauriConfig() {
  const configDir = path.join(root, ".iris-dev", "tmp");
  const configPath = path.join(configDir, "package-local-tauri.conf.json");
  const config = {
    bundle: {
      createUpdaterArtifacts: updaterSigningEnabled,
      macOS: {
        signingIdentity: "-",
      },
      resources: {
        [embeddedModelSource]: "models/bge-small-zh-v1.5",
      },
    },
  };
  mkdirSync(configDir, { recursive: true });
  writeFileSync(configPath, `${JSON.stringify(config, null, 2)}\n`, "utf8");
  return configPath;
}

function resetTargetBundle(target) {
  if (target === "mac") {
    rmSync(path.join(bundleRoot, "macos"), { force: true, recursive: true });
    rmSync(path.join(bundleRoot, "dmg"), { force: true, recursive: true });
    return;
  }
  rmSync(path.join(bundleRoot, "nsis"), { force: true, recursive: true });
  rmSync(path.join(releaseRoot, "nsis"), { force: true, recursive: true });
}

function prepareEmbeddedModel() {
  run("prepare embedded BGE model", "npm", ["run", "model:prepare"]);
}

function runEmbeddingAndSqliteVecSmoke() {
  run("smoke test embedded BGE model and sqlite-vec", "cargo", [
    "test",
    "--locked",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "--test",
    "embedding_model_smoke",
    "--",
    "--ignored",
  ]);
}

function smokeStatusLabel() {
  return process.env.IRIS_PACKAGE_SKIP_SMOKE === "1"
    ? "smoke skipped by IRIS_PACKAGE_SKIP_SMOKE=1"
    : "smoke verified";
}

function runChecks() {
  run("version check", "npm", ["run", "version:check"]);
  run("typecheck", "npm", ["run", "typecheck"]);
  run("targeted package/render tests", "npm", [
    "run",
    "test",
    "--",
    "tests/package-local-script-contract.test.ts",
    "tests/ai-code-copy.test.tsx",
    "tests/runtime-contracts.test.ts",
    "tests/trusted-types-production-regression.test.tsx",
  ]);
}

function createLocalDmg() {
  const version = packageVersion();
  const dmgDir = path.join(bundleRoot, "dmg");
  const stagingDir = path.join(root, ".iris-dev", "tmp", "package-local-dmg");
  const dmgPath = path.join(dmgDir, `Iris_${version}_${archLabel()}.dmg`);

  rmSync(stagingDir, { force: true, recursive: true });
  mkdirSync(stagingDir, { recursive: true });
  mkdirSync(dmgDir, { recursive: true });
  rmSync(dmgPath, { force: true });
  cpSync(appPath, path.join(stagingDir, "Iris.app"), { recursive: true });
  symlinkSync("/Applications", path.join(stagingDir, "Applications"));

  run("create local DMG", "hdiutil", [
    "create",
    "-srcfolder",
    stagingDir,
    "-format",
    "UDZO",
    "-volname",
    "Iris",
    dmgPath,
  ]);
  rmSync(stagingDir, { force: true, recursive: true });
  return dmgPath;
}

function packageMac() {
  if (process.platform !== "darwin") {
    throw new Error("mac packaging must run on macOS.");
  }
  prepareEmbeddedModel();
  if (process.env.IRIS_PACKAGE_SKIP_SMOKE !== "1") {
    runEmbeddingAndSqliteVecSmoke();
  }
  resetTargetBundle("mac");
  const configPath = writePackageTauriConfig();
  try {
    run(
      "build macOS app intermediate",
      "npm",
      tauriBuildArgs("mac", configPath),
    );
  } finally {
    rmSync(configPath, { force: true });
  }
  const dmgPath = createLocalDmg();
  run("verify desktop package", "node", [
    "scripts/verify-desktop-package.mjs",
    "mac",
  ]);
  process.stdout.write(
    [
      "",
      "[package-local] macOS DMG ready",
      `  path: ${dmgPath}`,
      `  version: ${packageVersion()}`,
      `  arch: ${archLabel()}`,
      `  sqlite-vec: required default feature, ${smokeStatusLabel()}`,
      `  trusted-types: ${trustedTypesStatus()}`,
      "  signing: ad-hoc app signature, unsigned DMG",
      "",
    ].join("\n"),
  );
}

function packageWin() {
  if (process.platform !== "win32") {
    throw new Error("Windows NSIS packaging must run on Windows.");
  }
  prepareEmbeddedModel();
  if (process.env.IRIS_PACKAGE_SKIP_SMOKE !== "1") {
    runEmbeddingAndSqliteVecSmoke();
  }
  resetTargetBundle("win");
  const configPath = writePackageTauriConfig();
  try {
    run(
      "build Windows NSIS installer",
      "npm",
      tauriBuildArgs("win", configPath),
    );
  } finally {
    rmSync(configPath, { force: true });
  }
  run("verify desktop package", "node", [
    "scripts/verify-desktop-package.mjs",
    "win",
  ]);
  process.stdout.write(
    [
      "",
      "[package-local] Windows NSIS build finished",
      "  installer: NSIS setup.exe",
      `  bundle dir: ${path.join(bundleRoot, "nsis")}`,
      `  version: ${packageVersion()}`,
      `  arch: ${archLabel()}`,
      `  sqlite-vec: required default feature, ${smokeStatusLabel()}`,
      `  trusted-types: ${trustedTypesStatus()}`,
      "  signing: unsigned self-use installer",
      "",
    ].join("\n"),
  );
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.check) runChecks();
  if (options.target === "mac") packageMac();
  else packageWin();
}

try {
  main();
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  console.error(`[package-local] ${message}`);
  process.exit(1);
}
