#!/usr/bin/env node
import {
  createReadStream,
  createWriteStream,
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createHash } from "node:crypto";
import path from "node:path";
import { pipeline } from "node:stream/promises";
import { Readable } from "node:stream";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const defaultManifestPath = path.join(
  root,
  "src-tauri",
  "resources",
  "embedding-models",
  "bge-small-zh-v1.5.manifest.json",
);
const defaultOutputPath = path.join(
  root,
  ".iris-dev",
  "models",
  "bge-small-zh-v1.5",
);
const readyFileName = ".iris-model-ready.json";

function usage() {
  return [
    "Usage:",
    "  node scripts/prepare-embedding-model.mjs [--offline] [--manifest <path>] [--output <path>]",
    "",
    "Downloads the pinned embedded BGE model only when a verified staging directory is unavailable.",
    "--offline verifies an existing staging directory and never performs network requests.",
  ].join("\n");
}

function parseArgs(argv) {
  const options = {
    manifestPath: defaultManifestPath,
    offline: false,
    outputPath: defaultOutputPath,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--offline") {
      options.offline = true;
      continue;
    }
    if (arg === "--manifest" || arg === "--output") {
      const value = argv[index + 1];
      if (!value || value.startsWith("--")) {
        throw new Error(`Missing value for ${arg}`);
      }
      if (arg === "--manifest") options.manifestPath = path.resolve(value);
      else options.outputPath = path.resolve(value);
      index += 1;
      continue;
    }
    if (arg === "--help" || arg === "-h") {
      process.stdout.write(`${usage()}\n`);
      process.exit(0);
    }
    throw new Error(`Unknown argument: ${arg}`);
  }

  return options;
}

function isSafeRelativePath(value) {
  return (
    typeof value === "string" &&
    value.length > 0 &&
    !path.isAbsolute(value) &&
    !value.split(/[\\/]/).includes("..")
  );
}

function validateManifest(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Model manifest must be a JSON object");
  }
  const manifest = value;
  if (manifest.schemaVersion !== 1) {
    throw new Error("Unsupported model manifest schemaVersion");
  }
  for (const field of ["id", "repository", "revision", "license"]) {
    if (typeof manifest[field] !== "string" || manifest[field].trim() === "") {
      throw new Error(`Model manifest requires a non-empty ${field}`);
    }
  }
  if (!/^[0-9a-f]{40}$/i.test(manifest.revision)) {
    throw new Error(
      "Model manifest revision must be an immutable 40-character Git commit",
    );
  }
  if (manifest.license !== "MIT") {
    throw new Error("Embedded model manifest license must be MIT");
  }
  if (!Array.isArray(manifest.files) || manifest.files.length === 0) {
    throw new Error("Model manifest requires at least one artifact");
  }

  const seenPaths = new Set();
  for (const file of manifest.files) {
    if (!file || typeof file !== "object") {
      throw new Error("Model manifest contains an invalid artifact");
    }
    if (!isSafeRelativePath(file.path)) {
      throw new Error(
        "Model manifest artifact path must be a safe relative path",
      );
    }
    if (seenPaths.has(file.path)) {
      throw new Error(
        `Model manifest contains a duplicate artifact path: ${file.path}`,
      );
    }
    seenPaths.add(file.path);
    if (typeof file.url !== "string" || !file.url.startsWith("https://")) {
      throw new Error(
        `Model manifest artifact URL must use HTTPS: ${file.path}`,
      );
    }
    if (file.sha256 !== undefined && !/^[0-9a-f]{64}$/i.test(file.sha256)) {
      throw new Error(`Model manifest has an invalid SHA-256 for ${file.path}`);
    }
    if (
      file.bytes !== undefined &&
      (!Number.isSafeInteger(file.bytes) || file.bytes <= 0)
    ) {
      throw new Error(
        `Model manifest has an invalid byte size for ${file.path}`,
      );
    }
  }

  const modelFile = manifest.files.find(
    (file) => file.path === "onnx/model.onnx",
  );
  if (!modelFile?.sha256) {
    throw new Error("Model manifest must pin onnx/model.onnx with SHA-256");
  }
  return manifest;
}

function loadManifest(manifestPath) {
  if (!existsSync(manifestPath)) {
    throw new Error(`Model manifest was not found: ${manifestPath}`);
  }
  try {
    return validateManifest(JSON.parse(readFileSync(manifestPath, "utf8")));
  } catch (error) {
    if (error instanceof SyntaxError) {
      throw new Error(`Model manifest is not valid JSON: ${manifestPath}`);
    }
    throw error;
  }
}

async function sha256File(filePath) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(filePath)) hash.update(chunk);
  return hash.digest("hex");
}

async function validateStaging(outputPath, manifest) {
  for (const file of manifest.files) {
    const target = path.join(outputPath, file.path);
    if (!existsSync(target)) {
      throw new Error(`Missing required model artifact: ${file.path}`);
    }
    if (file.bytes !== undefined) {
      const actualBytes = readFileSync(target).byteLength;
      if (actualBytes !== file.bytes) {
        throw new Error(
          `Byte size mismatch for ${file.path}: expected ${file.bytes}, got ${actualBytes}`,
        );
      }
    }
    if (file.sha256) {
      const actualHash = await sha256File(target);
      if (actualHash !== file.sha256.toLowerCase()) {
        throw new Error(`SHA-256 mismatch for ${file.path}`);
      }
    }
  }
}

function removeReadyMarker(outputPath) {
  rmSync(path.join(outputPath, readyFileName), { force: true });
}

function writeReadyMarker(outputPath, manifest) {
  const marker = {
    schemaVersion: 1,
    id: manifest.id,
    repository: manifest.repository,
    revision: manifest.revision,
    license: manifest.license,
    files: manifest.files.map(({ path: relativePath, sha256, bytes }) => ({
      path: relativePath,
      ...(sha256 ? { sha256 } : {}),
      ...(bytes ? { bytes } : {}),
    })),
  };
  writeFileSync(
    path.join(outputPath, readyFileName),
    `${JSON.stringify(marker, null, 2)}\n`,
    "utf8",
  );
}

async function downloadFile(url, target) {
  const response = await fetch(url, { redirect: "follow" });
  if (!response.ok || !response.body) {
    throw new Error(`Download failed (${response.status}) for ${url}`);
  }
  mkdirSync(path.dirname(target), { recursive: true });
  await pipeline(Readable.fromWeb(response.body), createWriteStream(target));
}

async function downloadToTemporaryDirectory(outputPath, manifest) {
  const parent = path.dirname(outputPath);
  const tempPath = path.join(
    parent,
    `.${path.basename(outputPath)}.partial-${process.pid}`,
  );
  rmSync(tempPath, { recursive: true, force: true });
  mkdirSync(tempPath, { recursive: true });

  try {
    for (const file of manifest.files) {
      process.stdout.write(`[model:prepare] downloading ${file.path}\n`);
      await downloadFile(file.url, path.join(tempPath, file.path));
    }
    await validateStaging(tempPath, manifest);
    writeReadyMarker(tempPath, manifest);
    rmSync(outputPath, { recursive: true, force: true });
    renameSync(tempPath, outputPath);
  } catch (error) {
    rmSync(tempPath, { recursive: true, force: true });
    throw error;
  }
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const manifest = loadManifest(options.manifestPath);
  removeReadyMarker(options.outputPath);

  try {
    await validateStaging(options.outputPath, manifest);
    writeReadyMarker(options.outputPath, manifest);
    process.stdout.write(
      `[model:prepare] verified ${manifest.id}@${manifest.revision} in ${options.outputPath}\n`,
    );
    return;
  } catch (error) {
    if (options.offline) {
      throw new Error(
        `Embedded model is not ready in offline mode: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }

  await downloadToTemporaryDirectory(options.outputPath, manifest);
  process.stdout.write(
    `[model:prepare] prepared ${manifest.id}@${manifest.revision} in ${options.outputPath}\n`,
  );
}

main().catch((error) => {
  const message = error instanceof Error ? error.message : String(error);
  console.error(`[model:prepare] ${message}`);
  process.exit(1);
});
