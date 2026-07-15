#!/usr/bin/env node
import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";

const SEMVER_PATTERN =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;
const PLATFORM_KEYS = ["darwin-aarch64", "windows-x86_64"];

function usage() {
  return [
    "Usage:",
    "  node scripts/verify-updater-release.mjs --version <semver> --asset-base-url <url> --assets-dir <dir> --manifest <latest.json>",
  ].join("\n");
}

function parseArgs(argv) {
  const options = {
    version: "",
    assetBaseUrl: "",
    assetsDir: "",
    manifest: "",
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = argv[index + 1];
    if (arg === "--version" && next) {
      options.version = next.replace(/^v/, "");
      index += 1;
      continue;
    }
    if (arg === "--asset-base-url" && next) {
      options.assetBaseUrl = next.replace(/\/$/, "");
      index += 1;
      continue;
    }
    if (arg === "--assets-dir" && next) {
      options.assetsDir = next;
      index += 1;
      continue;
    }
    if (arg === "--manifest" && next) {
      options.manifest = next;
      index += 1;
      continue;
    }
    if (arg === "--help" || arg === "-h") {
      process.stdout.write(`${usage()}\n`);
      process.exit(0);
    }
    throw new Error(`Unknown or incomplete argument: ${arg}`);
  }
  for (const [key, value] of Object.entries(options)) {
    if (!value) throw new Error(`Missing required option: ${key}`);
  }
  if (!SEMVER_PATTERN.test(options.version)) {
    throw new Error(`Updater version must be valid SemVer: ${options.version}`);
  }
  const assetBaseUrl = new URL(options.assetBaseUrl);
  if (assetBaseUrl.protocol !== "https:") {
    throw new Error("Updater asset base URL must use HTTPS");
  }
  return options;
}

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    return entry.isDirectory() ? walk(full) : [full];
  });
}

function findOne(files, predicate, label) {
  const matches = files.filter(predicate);
  if (matches.length !== 1) {
    throw new Error(`Expected exactly one ${label}, found ${matches.length}`);
  }
  return matches[0];
}

function signatureFor(asset) {
  const signaturePath = `${asset}.sig`;
  if (!existsSync(signaturePath)) {
    throw new Error(`Missing updater signature: ${signaturePath}`);
  }
  const signature = readFileSync(signaturePath, "utf8").trim();
  if (!signature) {
    throw new Error(`Updater signature is empty: ${signaturePath}`);
  }
  return signature;
}

function assetUrl(baseUrl, file) {
  return `${baseUrl}/${encodeURIComponent(path.basename(file))}`;
}

function readManifest(manifestPath) {
  if (!existsSync(manifestPath)) {
    throw new Error(`Updater manifest was not found: ${manifestPath}`);
  }
  let manifest;
  try {
    manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  } catch {
    throw new Error(`Updater manifest is not valid JSON: ${manifestPath}`);
  }
  if (!manifest || typeof manifest !== "object" || Array.isArray(manifest)) {
    throw new Error("Updater manifest must be a JSON object");
  }
  return manifest;
}

function verifyPlatform(manifest, key, asset, expectedSignature, baseUrl) {
  const platform = manifest.platforms[key];
  if (!platform || typeof platform !== "object") {
    throw new Error(`Updater manifest is missing platform: ${key}`);
  }
  const expectedUrl = assetUrl(baseUrl, asset);
  if (platform.url !== expectedUrl) {
    throw new Error(
      `Updater URL does not match release asset for ${key}: expected ${expectedUrl}`,
    );
  }
  if (platform.signature !== expectedSignature) {
    throw new Error(
      `Updater signature does not match release asset for ${key}`,
    );
  }
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const files = walk(options.assetsDir);
  const macDmg = findOne(files, (file) => file.endsWith(".dmg"), "macOS DMG");
  const macUpdater = findOne(
    files,
    (file) => file.endsWith(".app.tar.gz"),
    "macOS updater .app.tar.gz",
  );
  const winUpdater = findOne(
    files,
    (file) => file.endsWith("setup.exe"),
    "Windows NSIS setup.exe",
  );
  const manifest = readManifest(options.manifest);

  if (manifest.version !== options.version) {
    throw new Error(
      `Updater version mismatch: expected ${options.version}, got ${String(manifest.version)}`,
    );
  }
  if (typeof manifest.notes !== "string") {
    throw new Error("Updater manifest notes must be a string");
  }
  if (
    typeof manifest.pub_date !== "string" ||
    Number.isNaN(Date.parse(manifest.pub_date))
  ) {
    throw new Error("Updater manifest pub_date must be a valid timestamp");
  }
  if (
    !manifest.platforms ||
    typeof manifest.platforms !== "object" ||
    Array.isArray(manifest.platforms)
  ) {
    throw new Error("Updater manifest platforms must be an object");
  }
  const actualKeys = Object.keys(manifest.platforms);
  if (
    actualKeys.length !== PLATFORM_KEYS.length ||
    PLATFORM_KEYS.some((key, index) => actualKeys[index] !== key)
  ) {
    throw new Error(
      `Updater manifest platforms must be exactly: ${PLATFORM_KEYS.join(", ")}`,
    );
  }

  verifyPlatform(
    manifest,
    "darwin-aarch64",
    macUpdater,
    signatureFor(macUpdater),
    options.assetBaseUrl,
  );
  verifyPlatform(
    manifest,
    "windows-x86_64",
    winUpdater,
    signatureFor(winUpdater),
    options.assetBaseUrl,
  );

  process.stdout.write(
    `[updater-release] release assets verified for ${options.version}: ${path.basename(macDmg)}, ${path.basename(macUpdater)}, ${path.basename(winUpdater)}\n`,
  );
}

try {
  main();
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  console.error(`[updater-release] ${message}`);
  process.exit(1);
}
