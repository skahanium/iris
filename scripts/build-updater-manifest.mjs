#!/usr/bin/env node
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

function usage() {
  return [
    "Usage:",
    "  node scripts/build-updater-manifest.mjs --version <semver> --asset-base-url <url> --assets-dir <dir> --out <latest.json>",
  ].join("\n");
}

function parseArgs(argv) {
  const options = {
    version: "",
    assetBaseUrl: "",
    assetsDir: "",
    out: "",
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const next = argv[i + 1];
    if (arg === "--version" && next) {
      options.version = next.replace(/^v/, "");
      i += 1;
      continue;
    }
    if (arg === "--asset-base-url" && next) {
      options.assetBaseUrl = next.replace(/\/$/, "");
      i += 1;
      continue;
    }
    if (arg === "--assets-dir" && next) {
      options.assetsDir = next;
      i += 1;
      continue;
    }
    if (arg === "--out" && next) {
      options.out = next;
      i += 1;
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

function assetUrl(baseUrl, file) {
  return `${baseUrl}/${encodeURIComponent(path.basename(file))}`;
}

function signatureFor(asset) {
  const sig = `${asset}.sig`;
  if (!existsSync(sig)) {
    throw new Error(`Missing updater signature: ${sig}`);
  }
  return readFileSync(sig, "utf8").trim();
}

const options = parseArgs(process.argv.slice(2));
const files = walk(options.assetsDir);
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

const macPlatform = {
  signature: signatureFor(macUpdater),
  url: assetUrl(options.assetBaseUrl, macUpdater),
};
const winPlatform = {
  signature: signatureFor(winUpdater),
  url: assetUrl(options.assetBaseUrl, winUpdater),
};

const manifest = {
  version: options.version,
  notes: "",
  pub_date: new Date().toISOString(),
  platforms: {
    "darwin-aarch64": macPlatform,
    "darwin-aarch64/app": macPlatform,
    "windows-x86_64": winPlatform,
    "windows-x86_64/nsis": winPlatform,
  },
};

writeFileSync(options.out, `${JSON.stringify(manifest, null, 2)}\n`);
process.stdout.write(`[updater-manifest] wrote ${options.out}\n`);
