#!/usr/bin/env node
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const VERSION_PATTERN = String.raw`(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?`;
const VERSION_RE = new RegExp(`^${VERSION_PATTERN}$`);
const VERSION_TOKEN_RE = new RegExp(`v?${VERSION_PATTERN}`, "g");

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const defaultRoot = path.resolve(scriptDir, "..");

function usage() {
  return [
    "Usage:",
    "  node scripts/set-version.mjs <version>",
    "  node scripts/set-version.mjs --check [version]",
    "  node scripts/set-version.mjs --root <path> <version>",
    "",
    "Versions must be explicit SemVer such as 1.2.3 or 1.2.3-alpha.1.",
    "Do not prefix versions with v; display text adds v where needed.",
  ].join("\n");
}

function parseArgs(argv) {
  let root = defaultRoot;
  let check = false;
  const positional = [];

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--check") {
      check = true;
      continue;
    }
    if (arg === "--root") {
      const next = argv[index + 1];
      if (!next) throw new Error("--root requires a path");
      root = path.resolve(next);
      index += 1;
      continue;
    }
    if (arg === "--help" || arg === "-h") {
      process.stdout.write(`${usage()}\n`);
      process.exit(0);
    }
    if (arg.startsWith("--")) throw new Error(`Unknown option: ${arg}`);
    positional.push(arg);
  }

  if (positional.length > 1) {
    throw new Error(
      `Expected at most one version, received ${positional.length}`,
    );
  }

  return { check, root, version: positional[0] };
}

function validateVersion(version) {
  if (!VERSION_RE.test(version)) {
    throw new Error(
      `Invalid version "${version}". Use MAJOR.MINOR.PATCH, optionally with prerelease suffix.`,
    );
  }
}

function resolveFile(root, relativePath) {
  return path.join(root, relativePath);
}

function readText(root, relativePath) {
  const file = resolveFile(root, relativePath);
  if (!existsSync(file))
    throw new Error(`Missing required file: ${relativePath}`);
  return readFileSync(file, "utf8");
}

function writeText(root, relativePath, content) {
  writeFileSync(resolveFile(root, relativePath), content, "utf8");
}

function readJson(root, relativePath) {
  return JSON.parse(readText(root, relativePath));
}

function stableJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function updateJsonFile(root, relativePath, update) {
  const value = readJson(root, relativePath);
  update(value);
  return stableJson(value);
}

function replaceExactly(text, regex, replacement, label) {
  let count = 0;
  const next = text.replace(regex, (...args) => {
    count += 1;
    return typeof replacement === "function"
      ? replacement(...args)
      : replacement;
  });
  if (count !== 1) {
    throw new Error(`${label}: expected exactly 1 match, found ${count}`);
  }
  return next;
}

function versionedLine(line, version) {
  let count = 0;
  const next = line.replace(VERSION_TOKEN_RE, (token) => {
    count += 1;
    return token.startsWith("v") ? `v${version}` : version;
  });
  if (count === 0) throw new Error(`No version token found in line: ${line}`);
  return next;
}

function updateVersionedLine(text, regex, version, label) {
  return replaceExactly(
    text,
    regex,
    (line) => versionedLine(line, version),
    label,
  );
}

function updateCargoPackageVersion(text, version) {
  const regex = new RegExp(
    `(\\[package\\][\\s\\S]*?\\nversion\\s*=\\s*")${VERSION_PATTERN}(")`,
  );
  return replaceExactly(
    text,
    regex,
    (_match, prefix, suffix) => `${prefix}${version}${suffix}`,
    "src-tauri/Cargo.toml [package] version",
  );
}

function updateCargoLockPackageVersion(text, version) {
  const regex = new RegExp(
    `(\\[\\[package\\]\\]\\nname = "iris"\\nversion = ")${VERSION_PATTERN}(")`,
  );
  return replaceExactly(
    text,
    regex,
    (_match, prefix, suffix) => `${prefix}${version}${suffix}`,
    "src-tauri/Cargo.lock iris package version",
  );
}

function updatePackageJson(root, version) {
  return updateJsonFile(root, "package.json", (pkg) => {
    pkg.version = version;
  });
}

function updatePackageLock(root, version) {
  return updateJsonFile(root, "package-lock.json", (lock) => {
    lock.version = version;
    if (!lock.packages || !lock.packages[""]) {
      throw new Error("package-lock.json missing root package entry");
    }
    lock.packages[""].version = version;
  });
}

function updateTauriConfig(root, version) {
  return updateJsonFile(root, "src-tauri/tauri.conf.json", (config) => {
    config.version = version;
  });
}

const README_CURRENT_LINE_RE = new RegExp(
  `^\\*\\*[^\\r\\n]*\\*\\*[^\\r\\n]*(?:v?${VERSION_PATTERN})[^\\r\\n]*$`,
  "m",
);
const DOCS_README_CURRENT_RE = new RegExp(
  `^(\\*\\*v)${VERSION_PATTERN}(\\*\\*[^\\r\\n]*$)`,
  "m",
);
const ROADMAP_BASELINE_LINE_RE = new RegExp(
  `^.*\\*\\*v${VERSION_PATTERN}\\*\\*.*$`,
  "m",
);
const ROADMAP_CURRENT_HEADING_RE = new RegExp(
  `^##\\s+v${VERSION_PATTERN}[^\\r\\n]*$`,
  "m",
);
const CHANGELOG_CURRENT_HEADING_RE = new RegExp(
  `^(## \\[)${VERSION_PATTERN}(\\][^\\r\\n]*(?:Current|current)[^\\r\\n]*$)`,
  "m",
);
const ABOUT_VERSION_LINE_RE = /^.*GNU Affero General Public License v3\.0.*$/m;
const WEB_FETCH_USER_AGENT_RE = new RegExp(`(Iris/)${VERSION_PATTERN}`, "m");

function buildTextUpdaters(version) {
  return [
    {
      path: "src-tauri/Cargo.toml",
      update: (text) => updateCargoPackageVersion(text, version),
    },
    {
      path: "src-tauri/Cargo.lock",
      update: (text) => updateCargoLockPackageVersion(text, version),
    },
    {
      path: "README.md",
      update: (text) =>
        updateVersionedLine(
          text,
          README_CURRENT_LINE_RE,
          version,
          "README.md current version line",
        ),
    },
    {
      path: "docs/README.md",
      update: (text) =>
        replaceExactly(
          text,
          DOCS_README_CURRENT_RE,
          (_match, prefix, suffix) => `${prefix}${version}${suffix}`,
          "docs/README.md current version line",
        ),
    },
    {
      path: "ROADMAP.md",
      update: (text) => {
        const withBaseline = updateVersionedLine(
          text,
          ROADMAP_BASELINE_LINE_RE,
          version,
          "ROADMAP.md current baseline line",
        );
        return updateVersionedLine(
          withBaseline,
          ROADMAP_CURRENT_HEADING_RE,
          version,
          "ROADMAP.md current baseline heading",
        );
      },
    },
    {
      path: "CHANGELOG.md",
      update: (text) =>
        replaceExactly(
          text,
          CHANGELOG_CURRENT_HEADING_RE,
          (_match, prefix, suffix) => `${prefix}${version}${suffix}`,
          "CHANGELOG.md current heading",
        ),
    },
    {
      path: "src/components/settings/ManagementCenterPanel.tsx",
      update: (text) =>
        updateVersionedLine(
          text,
          ABOUT_VERSION_LINE_RE,
          version,
          "ManagementCenterPanel about version line",
        ),
    },
    {
      path: "src-tauri/src/llm/fetch_web_page.rs",
      update: (text) =>
        replaceExactly(
          text,
          WEB_FETCH_USER_AGENT_RE,
          (_match, prefix) => `${prefix}${version}`,
          "fetch_web_page.rs Iris user agent version",
        ),
    },
  ];
}

function buildUpdates(root, version) {
  return [
    { path: "package.json", next: updatePackageJson(root, version) },
    { path: "package-lock.json", next: updatePackageLock(root, version) },
    {
      path: "src-tauri/tauri.conf.json",
      next: updateTauriConfig(root, version),
    },
    ...buildTextUpdaters(version).map((entry) => ({
      path: entry.path,
      next: entry.update(readText(root, entry.path)),
    })),
  ];
}

function currentPackageVersion(root) {
  const pkg = readJson(root, "package.json");
  if (typeof pkg.version !== "string") {
    throw new Error("package.json version must be a string");
  }
  validateVersion(pkg.version);
  return pkg.version;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const version = args.version ?? currentPackageVersion(args.root);
  validateVersion(version);

  const updates = buildUpdates(args.root, version);
  const changed = updates.filter(({ path: relativePath, next }) => {
    const current = readText(args.root, relativePath);
    return current !== next;
  });

  if (args.check) {
    if (changed.length > 0) {
      process.stderr.write(
        `Version facts are not synchronized for ${version}:\n${changed
          .map((entry) => `- ${entry.path}`)
          .join("\n")}\n`,
      );
      process.exit(1);
    }
    process.stdout.write(`Version facts are synchronized for ${version}.\n`);
    return;
  }

  for (const entry of changed) {
    writeText(args.root, entry.path, entry.next);
  }

  if (changed.length === 0) {
    process.stdout.write(
      `Version facts already synchronized for ${version}.\n`,
    );
  } else {
    process.stdout.write(
      `Updated version facts to ${version}:\n${changed
        .map((entry) => `- ${entry.path}`)
        .join("\n")}\n`,
    );
  }
}

try {
  main();
} catch (error) {
  process.stderr.write(
    `${error instanceof Error ? error.message : String(error)}\n`,
  );
  process.stderr.write(`${usage()}\n`);
  process.exit(1);
}
