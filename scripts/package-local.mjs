#!/usr/bin/env node
import {
  existsSync,
  rmSync,
  mkdirSync,
  symlinkSync,
  readFileSync,
  cpSync,
} from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const bundleRoot = path.join(root, ".iris-dev", "target", "release", "bundle");
const appPath = path.join(bundleRoot, "macos", "Iris.app");

function usage() {
  return [
    "Usage:",
    "  node scripts/package-local.mjs [--check] [--sqlite-vec|--no-sqlite-vec] mac",
    "  node scripts/package-local.mjs [--check] [--sqlite-vec|--no-sqlite-vec] win",
    "",
    "Creates local self-use packages only. No Developer ID, notarization, CI, or Windows code signing.",
    "Windows defaults to sqlite-vec disabled; use --sqlite-vec for experimental vec0 builds.",
  ].join("\n");
}

function parseArgs(argv) {
  const options = {
    check: false,
    sqliteVec: null,
    target: null,
  };

  for (const arg of argv) {
    if (arg === "--check") {
      options.check = true;
      continue;
    }
    if (arg === "--sqlite-vec") {
      options.sqliteVec = true;
      continue;
    }
    if (arg === "--no-sqlite-vec") {
      options.sqliteVec = false;
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

  if (options.sqliteVec === null) {
    options.sqliteVec = options.target === "win" ? false : true;
  }

  return options;
}

function run(label, command, args) {
  process.stdout.write(`\n[package-local] ${label}\n`);
  const result = spawnSync(command, args, {
    cwd: root,
    shell: process.platform === "win32",
    stdio: "inherit",
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

function tauriBuildArgs(target, sqliteVec) {
  const args = ["run", "tauri", "--", "build"];
  if (sqliteVec) {
    args.push("--features", "sqlite-vec");
  }
  if (target === "mac") {
    args.push("--bundles", "app");
    return args;
  }
  args.push(
    "--config",
    "src-tauri/tauri.windows.conf.json",
    "--bundles",
    "nsis",
  );
  return args;
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

function signMacApp() {
  if (!existsSync(appPath)) {
    throw new Error(`macOS app bundle was not created: ${appPath}`);
  }
  run("ad-hoc sign Iris.app", "codesign", [
    "--force",
    "--deep",
    "--sign",
    "-",
    appPath,
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

function packageMac(options) {
  if (process.platform !== "darwin") {
    throw new Error("mac packaging must run on macOS.");
  }
  run(
    "build macOS app intermediate",
    "npm",
    tauriBuildArgs("mac", options.sqliteVec),
  );
  signMacApp();
  const dmgPath = createLocalDmg();
  process.stdout.write(
    [
      "",
      "[package-local] macOS DMG ready",
      `  path: ${dmgPath}`,
      `  version: ${packageVersion()}`,
      `  arch: ${archLabel()}`,
      `  sqlite-vec: ${options.sqliteVec ? "enabled" : "disabled"}`,
      `  trusted-types: ${trustedTypesStatus()}`,
      "  signing: ad-hoc app signature, unsigned DMG",
      "",
    ].join("\n"),
  );
}

function packageWin(options) {
  if (process.platform !== "win32") {
    throw new Error("Windows NSIS packaging must run on Windows.");
  }
  run(
    "build Windows NSIS installer",
    "npm",
    tauriBuildArgs("win", options.sqliteVec),
  );
  process.stdout.write(
    [
      "",
      "[package-local] Windows NSIS build finished",
      "  installer: NSIS setup.exe",
      `  bundle dir: ${path.join(bundleRoot, "nsis")}`,
      `  version: ${packageVersion()}`,
      `  arch: ${archLabel()}`,
      `  sqlite-vec: ${options.sqliteVec ? "enabled" : "disabled"}`,
      `  trusted-types: ${trustedTypesStatus()}`,
      "  signing: unsigned self-use installer",
      "",
    ].join("\n"),
  );
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.check) runChecks();
  if (options.target === "mac") packageMac(options);
  else packageWin(options);
}

try {
  main();
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  console.error(`[package-local] ${message}`);
  process.exit(1);
}
