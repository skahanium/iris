#!/usr/bin/env node
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
} from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const releaseRoot = path.join(root, ".iris-dev", "target", "release");
const bundleRoot = path.join(releaseRoot, "bundle");
const modelName = "bge-small-zh-v1.5";
const modelManifestPath = path.join(
  root,
  "src-tauri",
  "resources",
  "embedding-models",
  `${modelName}.manifest.json`,
);
const updaterSigningEnabled = Boolean(process.env.TAURI_SIGNING_PRIVATE_KEY);
const minimumWindowsInstallerBytes = 40 * 1024 * 1024;

function parseTarget(argv) {
  if (argv.length !== 1 || !["mac", "win"].includes(argv[0])) {
    throw new Error("Usage: node scripts/verify-desktop-package.mjs <mac|win>");
  }
  return argv[0];
}

function walk(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    return entry.isDirectory() ? walk(full) : [full];
  });
}

function walkDirectories(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    if (!entry.isDirectory()) return [];
    const full = path.join(dir, entry.name);
    return [full, ...walkDirectories(full)];
  });
}

function findOne(files, predicate, label) {
  const matches = files.filter(predicate);
  if (matches.length !== 1) {
    throw new Error(`Expected exactly one ${label}, found ${matches.length}`);
  }
  return matches[0];
}

function run(label, command, args) {
  const result = spawnSync(command, args, {
    cwd: root,
    encoding: "utf8",
    shell: process.platform === "win32",
  });
  if (result.status !== 0) {
    const detail = (result.stderr || result.stdout || "").trim();
    throw new Error(`${label} failed${detail ? `: ${detail}` : ""}`);
  }
  return result.stdout;
}

function modelManifest() {
  return JSON.parse(readFileSync(modelManifestPath, "utf8"));
}

function verifyModelDirectory(modelDir) {
  const manifest = modelManifest();
  for (const file of manifest.files) {
    const target = path.join(modelDir, file.path);
    if (!existsSync(target)) {
      throw new Error(`Packaged model artifact is missing: ${target}`);
    }
    if (file.bytes !== undefined && statSync(target).size !== file.bytes) {
      throw new Error(
        `Packaged model artifact has the wrong size: ${file.path}`,
      );
    }
  }
  const markerPath = path.join(modelDir, ".iris-model-ready.json");
  if (!existsSync(markerPath)) {
    throw new Error(
      `Packaged model readiness marker is missing: ${markerPath}`,
    );
  }
  const marker = JSON.parse(readFileSync(markerPath, "utf8"));
  if (marker.revision !== manifest.revision) {
    throw new Error("Packaged model readiness marker revision is stale");
  }
}

function verifyUpdaterSignature(asset) {
  const signaturePath = `${asset}.sig`;
  if (!existsSync(signaturePath)) {
    throw new Error(`Updater signature is missing: ${signaturePath}`);
  }
  if (!readFileSync(signaturePath, "utf8").trim()) {
    throw new Error(`Updater signature is empty: ${signaturePath}`);
  }
}

function verifyMacPackage() {
  if (process.platform !== "darwin") {
    throw new Error("macOS package verification must run on macOS");
  }
  const appPath = path.join(bundleRoot, "macos", "Iris.app");
  if (!existsSync(appPath)) {
    throw new Error(`macOS app bundle is missing: ${appPath}`);
  }
  verifyModelDirectory(
    path.join(appPath, "Contents", "Resources", "models", modelName),
  );
  run("macOS app signature verification", "codesign", [
    "--verify",
    "--deep",
    "--strict",
    appPath,
  ]);

  const dmg = findOne(
    walk(path.join(bundleRoot, "dmg")),
    (file) => file.endsWith(".dmg"),
    "macOS DMG",
  );
  if (statSync(dmg).size === 0) throw new Error(`macOS DMG is empty: ${dmg}`);

  if (!updaterSigningEnabled) return;
  const updater = findOne(
    walk(path.join(bundleRoot, "macos")),
    (file) => file.endsWith(".app.tar.gz"),
    "macOS updater .app.tar.gz",
  );
  verifyUpdaterSignature(updater);
  const archiveEntries = run("macOS updater archive listing", "tar", [
    "-tzf",
    updater,
  ]).replaceAll("\\", "/");
  for (const file of modelManifest().files) {
    const expected = `Contents/Resources/models/${modelName}/${file.path}`;
    if (!archiveEntries.includes(expected)) {
      throw new Error(`macOS updater archive is missing: ${expected}`);
    }
  }

  const tempBase = path.join(root, ".iris-dev", "tmp");
  mkdirSync(tempBase, { recursive: true });
  const extractRoot = mkdtempSync(path.join(tempBase, "verify-updater-"));
  try {
    run("macOS updater archive extraction", "tar", [
      "-xzf",
      updater,
      "-C",
      extractRoot,
    ]);
    const extractedApp = findOne(
      walkDirectories(extractRoot),
      (directory) => directory.endsWith(".app"),
      "app bundle in macOS updater archive",
    );
    verifyModelDirectory(
      path.join(extractedApp, "Contents", "Resources", "models", modelName),
    );
    run("macOS updater app signature verification", "codesign", [
      "--verify",
      "--deep",
      "--strict",
      extractedApp,
    ]);
  } finally {
    rmSync(extractRoot, { recursive: true, force: true });
  }
}

function verifyWindowsPackage() {
  if (process.platform !== "win32") {
    throw new Error("Windows package verification must run on Windows");
  }
  const installer = findOne(
    walk(path.join(bundleRoot, "nsis")),
    (file) => file.endsWith("setup.exe"),
    "Windows NSIS setup.exe",
  );
  if (statSync(installer).size < minimumWindowsInstallerBytes) {
    throw new Error(
      `Windows installer is unexpectedly small and likely missing the embedded model: ${statSync(installer).size} bytes`,
    );
  }
  if (updaterSigningEnabled) verifyUpdaterSignature(installer);

  const installerScript = findOne(
    walk(path.join(releaseRoot, "nsis")),
    (file) => path.basename(file) === "installer.nsi",
    "generated NSIS installer.nsi",
  );
  const normalizedScript = readFileSync(installerScript, "utf8").replaceAll(
    "\\",
    "/",
  );
  for (const file of [
    ...modelManifest().files.map((item) => item.path),
    ".iris-model-ready.json",
  ]) {
    const expected = `models/${modelName}/${file}`;
    if (!normalizedScript.includes(expected)) {
      throw new Error(`Generated NSIS installer is missing: ${expected}`);
    }
  }
}

try {
  const target = parseTarget(process.argv.slice(2));
  if (target === "mac") verifyMacPackage();
  else verifyWindowsPackage();
  process.stdout.write(`[package-verify] ${target} desktop package verified\n`);
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  console.error(`[package-verify] ${message}`);
  process.exit(1);
}
