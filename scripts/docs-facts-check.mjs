#!/usr/bin/env node
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(scriptDir, "..");

// ── CLI ────────────────────────────────────────────────────

const args = process.argv.slice(2);
let expectedMigrationGroups = null;
const forbiddenPhrases = [];

for (let i = 0; i < args.length; i += 1) {
  if (args[i] === "--expected-migration-group" && args[i + 1]) {
    expectedMigrationGroups = Number.parseInt(args[i + 1], 10);
    i += 1;
  } else if (args[i] === "--forbidden-phrase" && args[i + 1]) {
    forbiddenPhrases.push(args[i + 1]);
    i += 1;
  }
}

// ── Helpers ────────────────────────────────────────────────

const failures = [];

function fail(message) {
  failures.push(message);
}

function readJson(filePath) {
  return JSON.parse(readFileSync(filePath, "utf8"));
}

function walk(dir, predicate) {
  const entries = [];
  try {
    for (const entry of readdirSync(dir)) {
      const full = path.join(dir, entry);
      const stat = statSync(full);
      if (stat.isDirectory() && !entry.startsWith(".")) {
        entries.push(...walk(full, predicate));
      } else if (predicate(full)) {
        entries.push(full);
      }
    }
  } catch {
    // directory walk failure is fine
  }
  return entries;
}

// ── 1. Version consistency ─────────────────────────────────

function checkVersionConsistency() {
  const pkg = readJson(path.join(root, "package.json"));
  const cargoToml = readFileSync(
    path.join(root, "src-tauri", "Cargo.toml"),
    "utf8",
  );
  const tauriConf = readJson(path.join(root, "src-tauri", "tauri.conf.json"));

  const pkgVersion = pkg.version;
  const cargoMatch = cargoToml.match(/^version\s*=\s*"([^"]+)"/m);
  const cargoVersion = cargoMatch ? cargoMatch[1] : null;
  const tauriVersion = tauriConf.version;

  if (!pkgVersion) fail("package.json missing version field");
  if (!cargoVersion) fail("Cargo.toml missing version field");
  if (!tauriVersion) fail("tauri.conf.json missing version field");

  if (pkgVersion && cargoVersion && pkgVersion !== cargoVersion) {
    fail(
      `Version mismatch: package.json=${pkgVersion} vs Cargo.toml=${cargoVersion}`,
    );
  }
  if (pkgVersion && tauriVersion && pkgVersion !== tauriVersion) {
    fail(
      `Version mismatch: package.json=${pkgVersion} vs tauri.conf.json=${tauriVersion}`,
    );
  }

  const userAgentFile = path.join(
    root,
    "src-tauri",
    "src",
    "llm",
    "fetch_web_page.rs",
  );
  if (existsSync(userAgentFile) && pkgVersion) {
    const uaContent = readFileSync(userAgentFile, "utf8");
    if (!uaContent.includes(`Iris/${pkgVersion}`)) {
      fail(
        `User-Agent in llm/fetch_web_page.rs does not reference Iris/${pkgVersion}`,
      );
    }
  }
}

// ── 2. Migration count ─────────────────────────────────────

function checkMigrationCount() {
  const migrationsDir = path.join(root, "src-tauri", "migrations");
  const upFiles = readdirSync(migrationsDir).filter(
    (f) => f.endsWith(".sql") && !f.endsWith(".down.sql"),
  );
  const nums = upFiles
    .map((f) => {
      const m = f.match(/^(\d+)_/);
      return m ? Number.parseInt(m[1], 10) : 0;
    })
    .filter(Boolean);
  const maxNum = nums.length > 0 ? Math.max(...nums) : 0;
  const count = upFiles.length;

  const archPath = path.join(root, "ARCHITECTURE.md");
  const archContent = readFileSync(archPath, "utf8");
  const migrationLine = archContent
    .split("\n")
    .find((line) => line.includes("增量迁移") && line.includes("`001`"));

  if (!migrationLine) {
    fail("ARCHITECTURE.md missing migration count line");
    return;
  }

  const countMatch = migrationLine.match(/(\d+)\s*组/);
  const rangeMatch = migrationLine.match(/`0*1`\s*至\s*`(\d+)`/);

  if (!countMatch || !rangeMatch) {
    fail("ARCHITECTURE.md migration count line format not recognized");
    return;
  }

  const docCount = Number.parseInt(countMatch[1], 10);
  const docMax = Number.parseInt(rangeMatch[1], 10);

  const expected = expectedMigrationGroups ?? count;
  if (docCount !== expected || docMax !== maxNum) {
    fail(
      `ARCHITECTURE.md migration count: docs say ${docCount} 组 (001-${docMax}), actual is ${count} 组 (001-${maxNum})`,
    );
  }
}

// ── 3. Document links in docs/README.md ────────────────────

function checkDocLinks() {
  const indexContent = readFileSync(
    path.join(root, "docs", "README.md"),
    "utf8",
  );
  const linkRe = /\]\(\.\/([^)]+)\)/g;
  let match;
  while ((match = linkRe.exec(indexContent)) !== null) {
    const target = path.join(root, "docs", match[1]);
    if (!existsSync(target)) {
      fail(`docs/README.md links to missing file: ./${match[1]}`);
    }
  }
}

// ── 4. Forbidden phrases ───────────────────────────────────

function isNegationContext(line) {
  return /(?:不|禁止|无|没有|不做|不含|排除|免)/.test(line);
}

function lineContainsPhrase(line, phrase) {
  return line.includes(phrase) && !isNegationContext(line);
}

function checkForbiddenPhrases() {
  const phrases = forbiddenPhrases.length > 0 ? forbiddenPhrases : [];

  const docFiles = walk(path.join(root, "docs"), (f) => f.endsWith(".md"));
  const rootMdFiles = walk(root, (f) => {
    const rel = path.relative(root, f);
    return (
      f.endsWith(".md") &&
      !rel.startsWith("node_modules") &&
      !rel.startsWith("src-tauri") &&
      !rel.startsWith("src") &&
      !rel.startsWith(".git") &&
      !rel.startsWith(".worktrees") &&
      !rel.startsWith("iris-2.0-planning") &&
      !rel.startsWith("target")
    );
  });

  const allFiles = [...docFiles, ...rootMdFiles];

  for (const filePath of allFiles) {
    const lines = readFileSync(filePath, "utf8").split("\n");
    for (let i = 0; i < lines.length; i += 1) {
      for (const phrase of phrases) {
        if (lineContainsPhrase(lines[i], phrase)) {
          const rel = path.relative(root, filePath);
          fail(
            `Forbidden phrase "${phrase}" found in ${rel}:${i + 1}`,
          );
        }
      }
    }
  }

  // Check key docs for credential-manager promotion (not denial)
  for (const f of [
    path.join(root, "CONTRIBUTING.md"),
    path.join(root, "docs", "ipc-api-reference.md"),
    path.join(root, "docs", "ops", "performance-guide.md"),
  ]) {
    if (!existsSync(f)) continue;
    const lines = readFileSync(f, "utf8").split("\n");
    for (let i = 0; i < lines.length; i += 1) {
      if (lineContainsPhrase(lines[i], "OS 凭据管理器")) {
        fail(`${path.relative(root, f)}:${i + 1} — "OS 凭据管理器" (must say AES-256-GCM)`);
      }
    }
  }

  // Verify Skills descriptions: if they mention URL/Git/external install, it must be in denial context
  for (const f of [
    path.join(root, "README.md"),
    path.join(root, "ROADMAP.md"),
    path.join(root, "ARCHITECTURE.md"),
  ]) {
    if (!existsSync(f)) continue;
    const lines = readFileSync(f, "utf8").split("\n");
    for (let i = 0; i < lines.length; i += 1) {
      const ln = lines[i];
      if (!ln.toLowerCase().includes("skill")) continue;
      if (
        lineContainsPhrase(ln, "URL") ||
        lineContainsPhrase(ln, "Git") ||
        lineContainsPhrase(ln, "external")
      ) {
        fail(
          `${path.relative(root, f)}:${i + 1} — Skills line references URL/Git/external outside denial context`,
        );
      }
    }
  }
}

// ── 5. IPC command index ───────────────────────────────────

function checkIpcIndex() {
  const ipcRefPath = path.join(root, "docs", "ipc-api-reference.md");
  if (!existsSync(ipcRefPath)) return;

  // Verify search_embedding_status is documented
  const ipcContent = readFileSync(ipcRefPath, "utf8");
  if (!ipcContent.includes("search_embedding_status")) {
    fail("docs/ipc-api-reference.md missing search_embedding_status entry");
  }
  if (!ipcContent.includes("EmbeddingIndexStatus")) {
    fail("docs/ipc-api-reference.md missing EmbeddingIndexStatus reference");
  }
}

// ── Run ─────────────────────────────────────────────────────

checkVersionConsistency();
checkMigrationCount();
checkDocLinks();
checkForbiddenPhrases();
checkIpcIndex();

if (failures.length > 0) {
  process.stderr.write(`docs:check FAILED (${failures.length} issue(s)):\n`);
  for (const f of failures) {
    process.stderr.write(`  ✗ ${f}\n`);
  }
  process.exit(1);
}

process.stdout.write("docs:check PASSED\n");
process.exit(0);
